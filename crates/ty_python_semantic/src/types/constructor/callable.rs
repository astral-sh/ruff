//! Resumable conversion of a class constructor to callable signatures.

use smallvec::{SmallVec, smallvec_inline};

use super::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::place::{DefinedPlace, Place};
use crate::types::callable::CallableConversionRequest;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::enums::enum_metadata;
use crate::types::generics::GenericContext;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::{
    BoundMethodType, CallableType, CallableTypes, ClassType, DescriptorOrigin, MemberLookupPolicy,
    Type,
};
use crate::{Db, ProgramEnvironment};

/// Each pending operation consumes only the result type its continuation expects.
pub(in crate::types) enum ConstructorCallableStep<'db> {
    Member(PendingConstructorMember<'db>),
    Lookup(PendingConstructorLookup<'db>),
    BindInitializer(PendingInitializerBinding<'db>),
    Convert(PendingConstructorConversion<'db>),
    CheckNewReturn(PendingNewReturn<'db>),
    Complete(CallableTypes<'db>),
}

impl<'db> ConstructorCallableStep<'db> {
    pub(in crate::types) fn start(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        receiver: Type<'db>,
    ) -> Self {
        Self::Member(PendingConstructorMember {
            members: ConstructorMembers::new(db, env, class, receiver),
            stage: ConstructorMemberStage::MetaclassCall,
        })
    }
}

enum ConstructorMemberStage {
    MetaclassCall,
    New,
}

pub(in crate::types) struct PendingConstructorMember<'db> {
    members: ConstructorMembers<'db>,
    stage: ConstructorMemberStage,
}

impl<'db> PendingConstructorMember<'db> {
    pub(in crate::types) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorCallableStep<'db> {
        let member = match self.stage {
            ConstructorMemberStage::MetaclassCall => {
                self.members.metaclass_call(db, env, recursion_guard)
            }
            ConstructorMemberStage::New => self.members.new_method(db, env, recursion_guard),
        };
        self.resume(db, &member)
    }

    fn resume(
        self,
        db: &'db dyn Db,
        member: &ConstructorMember<'db>,
    ) -> ConstructorCallableStep<'db> {
        match self.stage {
            ConstructorMemberStage::MetaclassCall => {
                if let Place::Defined(DefinedPlace { ty, .. }) = member.place {
                    // TODO: this intentionally diverges from step 1 in
                    // https://typing.python.org/en/latest/spec/constructors.html#converting-a-constructor-to-callable
                    // by always respecting the signature of the metaclass `__call__`, rather than
                    // using a heuristic which makes unwarranted assumptions to sometimes ignore it.
                    //
                    // The only situation where we ignore the metaclass `__call__` is when the class is an actual enum
                    // (i.e. not a memberless superclass like `Enum`, `StrEnum`, etc.). In this case, we want to fall
                    // back to `Enum.__new__`/`StrEnum.__new__`/... which have more precise signatures for calls like
                    // `Color("red")`, instead of the overloaded signature of `EnumMeta.__call__` which also accounts
                    // for dynamic Enum creation.
                    let is_actual_enum =
                        enum_metadata(db, self.members.class.class_literal(db)).is_some();
                    if !is_actual_enum {
                        return ConstructorCallableStep::Convert(PendingConstructorConversion {
                            request: CallableConversionRequest::from_descriptor(ty, member.origin),
                            origin: member.origin,
                            continuation: ConstructorConversionContinuation::MetaclassCall(
                                self.members,
                            ),
                        });
                    }
                }
                Self::new_method(self.members)
            }
            ConstructorMemberStage::New => {
                if let Some(ty) = member.place.ignore_possibly_undefined() {
                    ConstructorCallableStep::Convert(PendingConstructorConversion {
                        request: CallableConversionRequest::from_descriptor(ty, member.origin),
                        origin: member.origin,
                        continuation: ConstructorConversionContinuation::New(self.members),
                    })
                } else {
                    PendingConstructorLookup::initializer(self.members, None)
                }
            }
        }
    }

    fn new_method(members: ConstructorMembers<'db>) -> ConstructorCallableStep<'db> {
        ConstructorCallableStep::Member(Self {
            members,
            stage: ConstructorMemberStage::New,
        })
    }
}

pub(in crate::types) enum PendingConstructorLookup<'db> {
    Initializer {
        members: ConstructorMembers<'db>,
        new_callables: Option<CallableTypes<'db>>,
    },
    ObjectNew {
        members: ConstructorMembers<'db>,
        class_generic_context: Option<GenericContext<'db>>,
    },
}

impl<'db> PendingConstructorLookup<'db> {
    pub(in crate::types) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        _recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorCallableStep<'db> {
        let place = match &self {
            Self::Initializer { members, .. } => members.raw_initializer(db, env, false),
            Self::ObjectNew { members, .. } => {
                Type::from(members.class)
                    .member_lookup_with_policy(
                        db,
                        env,
                        "__new__",
                        MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                    )
                    .place
            }
        };
        self.resume(db, env, place)
    }

    fn initializer(
        members: ConstructorMembers<'db>,
        new_callables: Option<CallableTypes<'db>>,
    ) -> ConstructorCallableStep<'db> {
        ConstructorCallableStep::Lookup(Self::Initializer {
            members,
            new_callables,
        })
    }

    fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        place: Place<'db>,
    ) -> ConstructorCallableStep<'db> {
        match self {
            Self::Initializer {
                members,
                new_callables,
            } => {
                if let Some(initializer) = place.ignore_possibly_undefined() {
                    InitializerSynthesis {
                        members,
                        new_callables,
                        remaining: smallvec_inline![initializer],
                        callables: SmallVec::new(),
                    }
                    .advance(db, env)
                } else {
                    finish_constructor(db, env, members, new_callables, None)
                }
            }
            Self::ObjectNew {
                members,
                class_generic_context,
            } => {
                if let Place::Defined(DefinedPlace {
                    ty: Type::FunctionLiteral(mut new_function),
                    ..
                }) = place
                {
                    if let Some(class_generic_context) = class_generic_context {
                        new_function =
                            new_function.with_inherited_generic_context(db, class_generic_context);
                    }
                    if let Some(callable) = new_function
                        .into_bound_method_type(db, members.instance)
                        .into_callable_type(db)
                    {
                        return ConstructorCallableStep::Complete(CallableTypes::one(callable));
                    }
                }

                // Fallback if no `object.__new__` is found.
                ConstructorCallableStep::Complete(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new_generic(
                        class_generic_context,
                        Parameters::empty(),
                        members.instance,
                    ),
                )))
            }
        }
    }
}

pub(in crate::types) struct PendingConstructorConversion<'db> {
    request: CallableConversionRequest<'db>,
    origin: DescriptorOrigin<'db>,
    continuation: ConstructorConversionContinuation<'db>,
}

enum ConstructorConversionContinuation<'db> {
    MetaclassCall(ConstructorMembers<'db>),
    New(ConstructorMembers<'db>),
    Initializer {
        synthesis: InitializerSynthesis<'db>,
        bound_method: Option<BoundMethodType<'db>>,
    },
}

impl<'db> PendingConstructorConversion<'db> {
    pub(in crate::types) fn request(&self) -> CallableConversionRequest<'db> {
        self.request
    }

    pub(in crate::types) fn origin(&self) -> DescriptorOrigin<'db> {
        self.origin
    }

    pub(in crate::types) fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callables: Option<CallableTypes<'db>>,
    ) -> ConstructorCallableStep<'db> {
        match self.continuation {
            ConstructorConversionContinuation::MetaclassCall(members) => {
                if let Some(callables) = callables {
                    ConstructorCallableStep::Complete(callables)
                } else {
                    PendingConstructorMember::new_method(members)
                }
            }
            ConstructorConversionContinuation::New(members) => {
                if let Some(callables) = callables {
                    let bound_callables = callables.map(|callable| {
                        callable.bind_self(db, env, members.receiver, members.instance)
                    });
                    CheckNewReturns {
                        members,
                        callables: bound_callables,
                        callable_index: 0,
                        signature_index: 0,
                    }
                    .advance(db)
                } else {
                    PendingConstructorLookup::initializer(members, None)
                }
            }
            ConstructorConversionContinuation::Initializer {
                mut synthesis,
                bound_method,
            } => {
                let Some(callables) = callables else {
                    return finish_constructor(
                        db,
                        env,
                        synthesis.members,
                        synthesis.new_callables,
                        None,
                    );
                };
                let callables = synthesis.synthesize_signatures(db, env, bound_method, callables);
                synthesis.callables.extend(callables.iter().copied());
                synthesis.advance(db, env)
            }
        }
    }
}

struct CheckNewReturns<'db> {
    members: ConstructorMembers<'db>,
    callables: CallableTypes<'db>,
    callable_index: usize,
    signature_index: usize,
}

impl<'db> CheckNewReturns<'db> {
    fn advance(mut self, db: &'db dyn Db) -> ConstructorCallableStep<'db> {
        while let Some(callable) = self.callables.iter().as_slice().get(self.callable_index) {
            if let Some(signature) = callable.signatures(db).overloads.get(self.signature_index) {
                self.signature_index += 1;
                return ConstructorCallableStep::CheckNewReturn(PendingNewReturn {
                    return_type: signature.return_ty,
                    continuation: self,
                });
            }
            self.callable_index += 1;
            self.signature_index = 0;
        }
        PendingConstructorLookup::initializer(self.members, Some(self.callables))
    }
}

pub(in crate::types) struct PendingNewReturn<'db> {
    return_type: Type<'db>,
    continuation: CheckNewReturns<'db>,
}

impl<'db> PendingNewReturn<'db> {
    pub(in crate::types) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        _recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorCallableStep<'db> {
        let is_assignable =
            self.return_type
                .is_assignable_to(db, env, self.continuation.members.instance);
        self.resume(db, is_assignable)
    }

    fn resume(self, db: &'db dyn Db, is_assignable: bool) -> ConstructorCallableStep<'db> {
        // Step 3: If the return type of the `__new__` evaluates to a type that is not a subclass of this class,
        // then we should ignore the `__init__` and just return the `__new__` method.
        if is_assignable {
            self.continuation.advance(db)
        } else {
            ConstructorCallableStep::Complete(self.continuation.callables)
        }
    }
}

/// Synthesizes constructor callables with the parameters of the bound `__init__` attribute
/// and the constructed instance as their return type.
struct InitializerSynthesis<'db> {
    members: ConstructorMembers<'db>,
    new_callables: Option<CallableTypes<'db>>,
    remaining: SmallVec<[Type<'db>; 1]>,
    callables: SmallVec<[CallableType<'db>; 1]>,
}

impl<'db> InitializerSynthesis<'db> {
    fn advance(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> ConstructorCallableStep<'db> {
        while let Some(initializer) = self.remaining.pop() {
            if let Some(union) = initializer.as_union_like(db) {
                // Expand alternatives in source order; a failed conversion discards the entire
                // initializer union before any later alternative is evaluated.
                self.remaining
                    .extend(union.elements(db).iter().rev().copied());
            } else {
                return ConstructorCallableStep::BindInitializer(PendingInitializerBinding {
                    synthesis: self,
                    initializer,
                });
            }
        }
        finish_constructor(
            db,
            env,
            self.members,
            self.new_callables,
            Some(CallableTypes::new(self.callables)),
        )
    }

    fn synthesize_signatures(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound_method: Option<BoundMethodType<'db>>,
        callables: CallableTypes<'db>,
    ) -> CallableTypes<'db> {
        let class_generic_context = self.members.class.constructor_generic_context(db, env);

        let synthesized_signature = |signature: &Signature<'db>| {
            let self_annotation = bound_method
                .filter(|method| !method.class_method(db))
                .and_then(|_| signature.parameters().get_positional(0))
                .filter(|parameter| !parameter.inferred_annotation)
                .map(Parameter::annotated_type)
                .filter(|ty| {
                    ty.as_typevar()
                        .is_none_or(|bound_typevar| !bound_typevar.typevar(db).is_self(db))
                });

            let mut signature = signature.clone();

            signature.generic_context = GenericContext::merge_optional(
                db,
                class_generic_context,
                signature.generic_context,
            );

            signature.return_ty = self_annotation.unwrap_or(self.members.instance);

            if let Some(method) = bound_method {
                // Constructor arguments determine the class's specialization, so
                // preserve generic parameters and overloads until they are checked.
                signature = signature.bind_self_with_receiver(
                    db,
                    env,
                    Some(method.signature_receiver(db)),
                    Some(method.typing_self_type(db)),
                );
            }
            signature.remove_unused_typevars(db, env)
        };

        callables.map(|callable| {
            let signatures = CallableSignature::from_overloads(
                callable.signatures(db).iter().map(synthesized_signature),
            );
            callable.with_signatures(db, signatures).into_regular(db)
        })
    }
}

pub(in crate::types) struct PendingInitializerBinding<'db> {
    synthesis: InitializerSynthesis<'db>,
    initializer: Type<'db>,
}

impl<'db> PendingInitializerBinding<'db> {
    pub(in crate::types) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> ConstructorCallableStep<'db> {
        let binding =
            self.synthesis
                .members
                .bind_initializer(db, env, self.initializer, recursion_guard);
        self.resume(binding)
    }

    fn resume(self, binding: InitializerBinding<'db>) -> ConstructorCallableStep<'db> {
        ConstructorCallableStep::Convert(PendingConstructorConversion {
            request: CallableConversionRequest::from_descriptor(binding.callable, binding.origin),
            origin: binding.origin,
            continuation: ConstructorConversionContinuation::Initializer {
                synthesis: self.synthesis,
                bound_method: binding.bound_method,
            },
        })
    }
}

fn finish_constructor<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    members: ConstructorMembers<'db>,
    new_callables: Option<CallableTypes<'db>>,
    init_callables: Option<CallableTypes<'db>>,
) -> ConstructorCallableStep<'db> {
    match (new_callables, init_callables) {
        (Some(new_callables), Some(init_callables)) => {
            ConstructorCallableStep::Complete(CallableTypes::from_elements(
                new_callables
                    .iter()
                    .copied()
                    .chain(init_callables.iter().copied()),
            ))
        }
        (Some(constructors), None) | (None, Some(constructors)) => {
            ConstructorCallableStep::Complete(constructors)
        }
        (None, None) => {
            let class_generic_context = members.class.constructor_generic_context(db, env);
            // If no `__new__` or `__init__` method is found, then we fall back to looking for
            // an `object.__new__` method.
            ConstructorCallableStep::Lookup(PendingConstructorLookup::ObjectNew {
                members,
                class_generic_context,
            })
        }
    }
}
