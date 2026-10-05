//! Resumable conversion of a class constructor to callable signatures.

use smallvec::{SmallVec, smallvec_inline};

use super::effects::{
    ConstructorCallableRequest, ConstructorEffects, ConstructorError, LegacyInlineEffects,
    inline_result,
};
use super::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::place::{DefinedPlace, Place};
use crate::types::callable::CallableConversionRequest;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::generics::GenericContext;
use crate::types::signatures::{CallableSignature, Parameters, Signature};
use crate::types::{
    BoundMethodType, CallableType, CallableTypes, ClassType, DescriptorOrigin, Type,
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
    ) -> Result<Self, ConstructorError> {
        inline_result(Self::start_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: None,
            },
            ConstructorCallableRequest { class, receiver },
        ))
    }

    async fn start_with<E: ConstructorEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        request: ConstructorCallableRequest<'db>,
    ) -> Result<Self, E::Error> {
        let instance = effects
            .instance_approximation(db, env, request.receiver)
            .await?
            .unwrap_or(Type::unknown());
        Ok(Self::Member(PendingConstructorMember {
            members: ConstructorMembers {
                class: request.class,
                receiver: request.receiver,
                instance,
            },
            stage: ConstructorMemberStage::MetaclassCall,
        }))
    }
}

/// The queued driver uses the same transitions as the synchronous conversion stack. Dependencies
/// suspend this future; their providers determine whether they are admitted or explicitly incomplete.
#[cfg(test)]
pub(in crate::types) async fn constructor_callables_with<'db, E: ConstructorEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    effects: &E,
    request: ConstructorCallableRequest<'db>,
) -> Result<CallableTypes<'db>, E::Error> {
    let mut step = ConstructorCallableStep::start_with(db, env, effects, request).await?;
    loop {
        step = match step {
            ConstructorCallableStep::Member(pending) => {
                pending.evaluate_with(db, env, effects).await?
            }
            ConstructorCallableStep::Lookup(pending) => {
                pending.evaluate_with(db, env, effects).await?
            }
            ConstructorCallableStep::BindInitializer(pending) => {
                pending.evaluate_with(db, env, effects).await?
            }
            ConstructorCallableStep::Convert(pending) => {
                let callables = effects
                    .convert(db, env, pending.request(), pending.origin())
                    .await?;
                pending.resume_with(db, env, effects, callables).await?
            }
            ConstructorCallableStep::CheckNewReturn(pending) => {
                pending.evaluate_with(db, env, effects).await?
            }
            ConstructorCallableStep::Complete(callables) => return Ok(callables),
        };
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
    ) -> Result<ConstructorCallableStep<'db>, ConstructorError> {
        inline_result(self.evaluate_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: Some(recursion_guard),
            },
        ))
    }

    async fn evaluate_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        let member = match self.stage {
            ConstructorMemberStage::MetaclassCall => {
                effects.metaclass_call(db, env, self.members).await?
            }
            ConstructorMemberStage::New => effects.new_method(db, env, self.members).await?,
        };
        self.resume_with(db, env, effects, &member).await
    }

    async fn resume_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        member: &ConstructorMember<'db>,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        Ok(match self.stage {
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
                        effects.is_actual_enum(db, env, self.members.class).await?;
                    if !is_actual_enum {
                        return Ok(ConstructorCallableStep::Convert(
                            PendingConstructorConversion {
                                request: CallableConversionRequest::from_descriptor(
                                    ty,
                                    member.origin,
                                ),
                                origin: member.origin,
                                continuation: ConstructorConversionContinuation::MetaclassCall(
                                    self.members,
                                ),
                            },
                        ));
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
        })
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
    ) -> Result<ConstructorCallableStep<'db>, ConstructorError> {
        inline_result(self.evaluate_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: None,
            },
        ))
    }

    async fn evaluate_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        let place = match &self {
            Self::Initializer { members, .. } => effects.raw_initializer(db, env, *members).await?,
            Self::ObjectNew { members, .. } => effects.object_new(db, env, *members).await?,
        };
        self.resume_with(db, env, effects, place).await
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

    async fn resume_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        place: Place<'db>,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
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
                    .advance_with(db, env, effects)
                    .await
                } else {
                    finish_constructor_with(db, env, effects, members, new_callables, None).await
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
                        new_function = effects
                            .specialize_object_new(db, env, new_function, class_generic_context)
                            .await?;
                    }
                    if let Some(callable) = effects
                        .object_new_callable(db, env, new_function, members.instance)
                        .await?
                    {
                        return Ok(ConstructorCallableStep::Complete(CallableTypes::one(
                            callable,
                        )));
                    }
                }

                // Fallback if no `object.__new__` is found.
                Ok(ConstructorCallableStep::Complete(CallableTypes::one(
                    CallableType::single(
                        db,
                        Signature::new_generic(
                            class_generic_context,
                            Parameters::empty(),
                            members.instance,
                        ),
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
    ) -> Result<ConstructorCallableStep<'db>, ConstructorError> {
        inline_result(self.resume_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: None,
            },
            callables,
        ))
    }

    async fn resume_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        callables: Option<CallableTypes<'db>>,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        match self.continuation {
            ConstructorConversionContinuation::MetaclassCall(members) => {
                Ok(if let Some(callables) = callables {
                    ConstructorCallableStep::Complete(callables)
                } else {
                    PendingConstructorMember::new_method(members)
                })
            }
            ConstructorConversionContinuation::New(members) => {
                if let Some(callables) = callables {
                    let mut bound_callables = SmallVec::with_capacity(callables.iter().len());
                    for callable in &callables {
                        bound_callables.push(
                            effects
                                .bind_new_self(
                                    db,
                                    env,
                                    *callable,
                                    members.receiver,
                                    members.instance,
                                )
                                .await?,
                        );
                    }
                    Ok(CheckNewReturns {
                        members,
                        callables: CallableTypes::new(bound_callables),
                        callable_index: 0,
                        signature_index: 0,
                    }
                    .advance(db))
                } else {
                    Ok(PendingConstructorLookup::initializer(members, None))
                }
            }
            ConstructorConversionContinuation::Initializer {
                mut synthesis,
                bound_method,
            } => {
                let Some(callables) = callables else {
                    return finish_constructor_with(
                        db,
                        env,
                        effects,
                        synthesis.members,
                        synthesis.new_callables,
                        None,
                    )
                    .await;
                };
                let callables = synthesis
                    .synthesize_signatures_with(db, env, effects, bound_method, callables)
                    .await?;
                synthesis.callables.extend(callables.iter().copied());
                synthesis.advance_with(db, env, effects).await
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
    ) -> Result<ConstructorCallableStep<'db>, ConstructorError> {
        inline_result(self.evaluate_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: None,
            },
        ))
    }

    async fn evaluate_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        let is_assignable = effects
            .new_return_assignable(
                db,
                env,
                self.return_type,
                self.continuation.members.instance,
            )
            .await?;
        Ok(self.resume(db, is_assignable))
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
    async fn advance_with<E: ConstructorEffects<'db>>(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        while let Some(initializer) = self.remaining.pop() {
            if let Some(union) = effects.expand_initializer(db, env, initializer).await? {
                // Expand alternatives in source order; a failed conversion discards the entire
                // initializer union before any later alternative is evaluated.
                self.remaining
                    .extend(union.elements(db).iter().rev().copied());
            } else {
                return Ok(ConstructorCallableStep::BindInitializer(
                    PendingInitializerBinding {
                        synthesis: self,
                        initializer,
                    },
                ));
            }
        }
        finish_constructor_with(
            db,
            env,
            effects,
            self.members,
            self.new_callables,
            Some(CallableTypes::new(self.callables)),
        )
        .await
    }

    async fn synthesize_signatures_with<E: ConstructorEffects<'db>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        bound_method: Option<BoundMethodType<'db>>,
        callables: CallableTypes<'db>,
    ) -> Result<CallableTypes<'db>, E::Error> {
        let class_generic_context = effects
            .class_generic_context(db, env, self.members.class)
            .await?;
        let mut synthesized = SmallVec::with_capacity(callables.iter().len());
        for callable in &callables {
            let signatures = callable.signatures(db);
            let mut overloads = SmallVec::with_capacity(signatures.overloads.len());
            for signature in signatures {
                let self_annotation = effects
                    .initializer_self_annotation(db, env, bound_method, signature)
                    .await?;
                let mut signature = signature.clone();
                signature.generic_context = effects
                    .merge_generic_context(
                        db,
                        env,
                        class_generic_context,
                        signature.generic_context,
                    )
                    .await?;
                signature.return_ty = self_annotation.unwrap_or(self.members.instance);

                if let Some(method) = bound_method {
                    // Constructor arguments determine the class's specialization, so
                    // preserve generic parameters and overloads until they are checked.
                    signature = effects
                        .bind_initializer_signature(db, env, signature, method)
                        .await?;
                }
                overloads.push(effects.remove_unused_typevars(db, env, signature).await?);
            }
            let signatures = CallableSignature { overloads };
            synthesized.push(callable.with_signatures(db, signatures).into_regular(db));
        }
        Ok(CallableTypes::new(synthesized))
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
    ) -> Result<ConstructorCallableStep<'db>, ConstructorError> {
        inline_result(self.evaluate_with(
            db,
            env,
            &LegacyInlineEffects {
                recursion_guard: Some(recursion_guard),
            },
        ))
    }

    async fn evaluate_with<E: ConstructorEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<ConstructorCallableStep<'db>, E::Error> {
        let binding = effects
            .bind_initializer(db, env, self.synthesis.members, self.initializer)
            .await?;
        Ok(self.resume(binding))
    }

    fn resume(self, binding: InitializerBinding<'db>) -> ConstructorCallableStep<'db> {
        #[cfg(test)]
        super::expansion_probe::observe(
            super::expansion_probe::Observation::InitializerBindingResumed,
        );
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

async fn finish_constructor_with<'db, E: ConstructorEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    effects: &E,
    members: ConstructorMembers<'db>,
    new_callables: Option<CallableTypes<'db>>,
    init_callables: Option<CallableTypes<'db>>,
) -> Result<ConstructorCallableStep<'db>, E::Error> {
    Ok(match (new_callables, init_callables) {
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
            let class_generic_context = effects
                .class_generic_context(db, env, members.class)
                .await?;
            // If no `__new__` or `__init__` method is found, then we fall back to looking for
            // an `object.__new__` method.
            ConstructorCallableStep::Lookup(PendingConstructorLookup::ObjectNew {
                members,
                class_generic_context,
            })
        }
    })
}
