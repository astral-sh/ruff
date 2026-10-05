//! Callable conversion with owned continuations for its semantic dependencies.

use smallvec::SmallVec;

use super::{
    CallableConversionRequest, CallableType, CallableTypeKind, CallableTypes, UpcastPolicy,
};
use crate::place::Place;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::known_instance::MethodWrapperKind;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::{
    BoundMethodType, ClassType, DescriptorOrigin, KnownBoundMethodType, KnownInstanceType,
    LiteralValueTypeKind, MemberLookupPolicy, ResolvedMember, SubclassOfInner, Type,
    TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

pub(super) enum ConversionStep<'db> {
    Convert(PendingConversion<'db>),
    Constructor {
        class: ClassType<'db>,
        receiver: Type<'db>,
    },
    CallMember(PendingCallMember<'db>),
    CachedBoundMethod(BoundMethodType<'db>),
    Complete(Option<CallableTypes<'db>>),
}

pub(super) fn start<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: CallableConversionRequest<'db>,
    has_recursion_guard: bool,
) -> ConversionStep<'db> {
    if let Some(fallback) = request.ty.materialized_divergent_fallback() {
        return request.convert(fallback, ConversionContinuation::Identity);
    }

    let complete = |callable| ConversionStep::Complete(Some(CallableTypes::one(callable)));
    match request.ty {
        Type::RecursiveVar(_) => {
            unreachable!("semantic operation on an unbound recursive variable")
        }
        Type::Callable(callable) => complete(callable),
        Type::Dynamic(_) => {
            let signature = if request.ty.is_unknown() && request.unknown_is_recovery {
                Signature::recursion_recovery()
            } else {
                Signature::dynamic(request.ty)
            };
            complete(CallableType::function_like(db, signature))
        }
        Type::Divergent(_) => complete(CallableType::function_like(
            db,
            Signature::dynamic(request.ty),
        )),
        Type::Recursive(recursive) => {
            let Some(unfolded) = recursive.unfold(db, env).into_unfolded() else {
                return ConversionStep::Complete(None);
            };
            request.convert(unfolded, ConversionContinuation::Identity)
        }
        Type::FunctionLiteral(function) if request.is_recursive_reference(db, function) => {
            complete(CallableType::bottom(db))
        }
        Type::FunctionLiteral(function) => complete(function.into_callable_type(db)),
        Type::BoundMethod(method)
            if method
                .function(db)
                .is_some_and(|function| request.is_recursive_reference(db, function)) =>
        {
            complete(CallableType::bottom(db))
        }
        Type::BoundMethod(method) => {
            if has_recursion_guard {
                request.convert(
                    method.func(db),
                    ConversionContinuation::BindReceiver(method),
                )
            } else {
                ConversionStep::CachedBoundMethod(method)
            }
        }
        Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
            ConversionStep::CallMember(PendingCallMember { request })
        }
        Type::ClassLiteral(class_literal) => {
            let class = class_literal.identity_specialization(db);
            ConversionStep::Constructor {
                class,
                receiver: Type::from(class),
            }
        }
        Type::GenericAlias(alias) => ConversionStep::Constructor {
            class: ClassType::Generic(alias),
            receiver: request.ty,
        },
        Type::NewTypeInstance(newtype) => request.convert(
            newtype.concrete_base_type(db),
            ConversionContinuation::Identity,
        ),
        Type::SubclassOf(subclass) if request.policy == UpcastPolicy::Sound => {
            complete(CallableType::function_like(
                db,
                Signature::new(Parameters::top(), subclass.to_instance(db, env)),
            ))
        }
        // TODO: This is unsound so in future we can consider an opt-in option to disable it.
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Class(class) => ConversionStep::Constructor {
                class,
                receiver: Type::from(class),
            },
            SubclassOfInner::Protocol(protocol) => {
                let Some(origin) = protocol.class_origin(db) else {
                    return ConversionStep::Complete(None);
                };
                let receiver = if protocol.materialization_kind(db).is_some() {
                    // The origin supplies the constructor, but the actual receiver retains
                    // `Top[P]` or `Bottom[P]`. Infer with both so instance-returning overloads
                    // are materialized without replacing explicit non-instance returns.
                    request.ty
                } else {
                    Type::from(*origin)
                };
                ConversionStep::Constructor {
                    class: *origin,
                    receiver,
                }
            }
            SubclassOfInner::TypeVar(typevar) => {
                match typevar.require_bound_or_constraints(db, env) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => request.convert(
                        bound.constructor_for_typevar_bound(db, env),
                        ConversionContinuation::ReplaceReturn(Type::TypeVar(typevar)),
                    ),
                    TypeVarBoundOrConstraints::Constraints(constraints) => ConversionAlternatives {
                        request,
                        remaining: constraints.elements(db),
                        return_type: Some(Type::TypeVar(typevar)),
                        callables: SmallVec::new(),
                    }
                    .advance(db, env),
                }
            }
            SubclassOfInner::Dynamic(_) => complete(CallableType::single(
                db,
                Signature::new(Parameters::unknown(), Type::from(subclass)),
            )),
        },
        Type::Union(union) => ConversionAlternatives {
            request,
            remaining: union.elements(db),
            return_type: None,
            callables: SmallVec::new(),
        }
        .advance(db, env),
        Type::LiteralValue(literal) => match literal.kind() {
            LiteralValueTypeKind::Enum(enum_literal) => request.convert(
                enum_literal.enum_class_instance(db, env),
                ConversionContinuation::Identity,
            ),
            _ => ConversionStep::Complete(None),
        },
        Type::TypeAlias(alias) => {
            request.convert(alias.value_type(db), ConversionContinuation::Identity)
        }
        Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => {
            request.convert(callable.inner(db), ConversionContinuation::Regularize)
        }
        Type::KnownBoundMethod(method) => ConversionStep::Complete(method.callables(db, env)),
        Type::WrapperDescriptor(wrapper) => complete(CallableType::new(
            db,
            CallableSignature::from_overloads(wrapper.signatures(db, env)),
            CallableTypeKind::Regular,
        )),
        Type::KnownInstance(KnownInstanceType::NewType(newtype)) => complete(CallableType::single(
            db,
            Signature::new(
                Parameters::standard([Parameter::positional_only(None)
                    .with_annotated_type(newtype.base(db).instance_type(db, env))]),
                Type::NewTypeInstance(newtype),
            ),
        )),
        Type::Never
        | Type::DataclassTransformer(_)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypeForm(_)
        | Type::TypedDict(_) => ConversionStep::Complete(None),
        Type::KnownInstance(
            KnownInstanceType::FunctoolsPartial(partial)
            | KnownInstanceType::FunctoolsPartialCall(partial),
        ) => complete(partial.partial(db)),
        Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => match wrapper.kind(db) {
            MethodWrapperKind::Staticmethod => {
                request.convert(wrapper.wrapped(db), ConversionContinuation::Identity)
            }
            MethodWrapperKind::Classmethod => ConversionStep::Complete(None),
        },
        Type::Intersection(intersection) => {
            let Some(alternatives) = intersection.finite_alternative_union(db, env) else {
                return ConversionStep::Complete(None);
            };
            request.convert(alternatives, ConversionContinuation::Identity)
        }
        Type::EnumComplement(complement) => request.convert(
            complement.remaining_literal_union(db, env),
            ConversionContinuation::Identity,
        ),
        // TODO
        Type::DataclassDecorator(_)
        | Type::ModuleLiteral(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::SlotDescriptor(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_) => ConversionStep::Complete(None),
    }
}

impl<'db> CallableConversionRequest<'db> {
    fn convert(
        self,
        ty: Type<'db>,
        continuation: ConversionContinuation<'db>,
    ) -> ConversionStep<'db> {
        ConversionStep::Convert(PendingConversion {
            request: Self { ty, ..self },
            origin: None,
            continuation,
        })
    }
}

pub(super) struct PendingConversion<'db> {
    pub(super) request: CallableConversionRequest<'db>,
    pub(super) origin: Option<DescriptorOrigin<'db>>,
    continuation: ConversionContinuation<'db>,
}

enum ConversionContinuation<'db> {
    Identity,
    Regularize,
    BindReceiver(BoundMethodType<'db>),
    ReplaceReturn(Type<'db>),
    Alternatives(ConversionAlternatives<'db>),
}

impl<'db> PendingConversion<'db> {
    pub(super) fn resume(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callables: Option<CallableTypes<'db>>,
    ) -> ConversionStep<'db> {
        let Some(callables) = callables else {
            return ConversionStep::Complete(None);
        };
        let callables = match self.continuation {
            ConversionContinuation::Identity => callables,
            ConversionContinuation::Regularize => {
                // The callable instance itself doesn't inherit the descriptor behavior of
                // its `__call__` method.
                callables.map(|callable| callable.into_regular(db))
            }
            ConversionContinuation::BindReceiver(method) => callables.map(|callable| {
                callable.bind_self(
                    db,
                    env,
                    method.signature_receiver(db),
                    method.typing_self_type(db),
                )
            }),
            ConversionContinuation::ReplaceReturn(return_type) => {
                callables.map(|callable| with_return_type(db, callable, return_type))
            }
            ConversionContinuation::Alternatives(mut alternatives) => {
                for callable in callables.into_inner() {
                    let callable = match alternatives.return_type {
                        Some(return_type) => with_return_type(db, callable, return_type),
                        None => callable,
                    };
                    alternatives.callables.push(callable);
                }
                return alternatives.advance(db, env);
            }
        };
        ConversionStep::Complete(Some(callables))
    }
}

struct ConversionAlternatives<'db> {
    request: CallableConversionRequest<'db>,
    remaining: &'db [Type<'db>],
    // A constrained type variable uses each constraint's constructor and retains the type
    // variable as the result. An ordinary union converts its elements without these changes.
    return_type: Option<Type<'db>>,
    callables: SmallVec<[CallableType<'db>; 1]>,
}

impl<'db> ConversionAlternatives<'db> {
    fn advance(mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> ConversionStep<'db> {
        let Some((&element, remaining)) = self.remaining.split_first() else {
            return ConversionStep::Complete(Some(CallableTypes::new(self.callables)));
        };
        self.remaining = remaining;
        let ty = if self.return_type.is_some() {
            element.to_meta_type(db, env)
        } else {
            element
        };
        self.request
            .convert(ty, ConversionContinuation::Alternatives(self))
    }
}

pub(super) struct PendingCallMember<'db> {
    request: CallableConversionRequest<'db>,
}

impl<'db> PendingCallMember<'db> {
    pub(super) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> ConversionStep<'db> {
        let member = self
            .request
            .ty
            .member_lookup_with_recursion_guard(
                db,
                env,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                None,
                recursion_guard,
            )
            .unwrap_or_else(|error| error.fallback_member(db));
        self.resume(db, member)
    }

    fn resume(self, db: &'db dyn Db, member: ResolvedMember<'db>) -> ConversionStep<'db> {
        let Place::Defined(place) = member.member(db).place else {
            return ConversionStep::Complete(None);
        };
        if !place.is_definitely_defined() {
            return ConversionStep::Complete(None);
        }
        let origin = member.descriptor_origin(db);
        ConversionStep::Convert(PendingConversion {
            request: CallableConversionRequest {
                ty: place.ty,
                unknown_is_recovery: origin.return_contains_recursive_recovery,
                ..self.request
            },
            origin: Some(origin),
            continuation: ConversionContinuation::Regularize,
        })
    }
}

fn with_return_type<'db>(
    db: &'db dyn Db,
    callable: CallableType<'db>,
    return_type: Type<'db>,
) -> CallableType<'db> {
    let signatures = callable
        .signatures(db)
        .into_iter()
        .map(|signature| signature.clone().with_return_type(return_type));
    callable.with_signatures(db, CallableSignature::from_overloads(signatures))
}
