//! Callable conversion with owned continuations for its semantic dependencies.

use smallvec::SmallVec;
use std::convert::Infallible;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{
    CallableConversionOperation, CallableConversionRequest, CallableType, CallableTypeKind,
    CallableTypes, UpcastPolicy,
};
use crate::place::Place;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::function::{FunctionMetadataEffects, LegacyFunctionIdentityEffects};
use crate::types::known_instance::MethodWrapperKind;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::{
    BoundMethodType, ClassType, DescriptorOrigin, FunctionType, KnownBoundMethodType,
    KnownInstanceType, LiteralValueTypeKind, MemberLookupPolicy, MemberLookupResult,
    ResolvedMember, SubclassOfInner, SubclassOfType, Type, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) enum ConversionStep<'db> {
    Convert(PendingConversion<'db>),
    Function(PendingFunction<'db>),
    SubclassInstance(SubclassOfType<'db>),
    RuntimeUnion(RuntimeUnionConversion<'db>),
    Constructor {
        class: ClassType<'db>,
        receiver: Type<'db>,
    },
    CallMember(PendingCallMember<'db>),
    CachedBoundMethod(BoundMethodType<'db>),
    Complete(Option<CallableTypes<'db>>),
}

pub(in crate::types) trait ConversionAdmission {
    type Error;

    fn checkpoint(&self) -> Result<(), Self::Error>;
    fn dependency(&self, operation: CallableConversionOperation) -> Result<(), Self::Error>;
}

struct InlineConversionAdmission;

impl ConversionAdmission for InlineConversionAdmission {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn dependency(&self, _operation: CallableConversionOperation) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(in crate::types) trait FunctionConversionEffects<'db>:
    FunctionMetadataEffects<'db>
{
    async fn local<T>(
        &self,
        work: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error>;

    async fn callable(
        &self,
        db: &'db dyn Db,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
    ) -> Result<CallableType<'db>, Self::Error>;
}

impl<'db> FunctionConversionEffects<'db> for LegacyFunctionIdentityEffects {
    async fn local<T>(
        &self,
        _work: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn signature(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(function.signature(db))
    }

    async fn callable(
        &self,
        db: &'db dyn Db,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
    ) -> Result<CallableType<'db>, Self::Error> {
        Ok(CallableType::new(db, signatures, kind))
    }
}

pub(super) fn start<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: CallableConversionRequest<'db>,
    has_recursion_guard: bool,
) -> ConversionStep<'db> {
    match start_with(
        db,
        env,
        request,
        has_recursion_guard,
        &InlineConversionAdmission,
    ) {
        Ok(step) => step,
        Err(never) => match never {},
    }
}

pub(in crate::types) fn start_with<'db, A: ConversionAdmission>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: CallableConversionRequest<'db>,
    has_recursion_guard: bool,
    admission: &A,
) -> Result<ConversionStep<'db>, A::Error> {
    admission.checkpoint()?;
    if let Some(fallback) = request.ty.materialized_divergent_fallback() {
        return Ok(request.convert(
            fallback,
            ConversionContinuation::Transform(ConversionTransform::Identity),
        ));
    }

    let complete = |callable| ConversionStep::Complete(Some(CallableTypes::one(callable)));
    Ok(match request.ty {
        Type::RecursiveVar(_) => {
            unreachable!("semantic operation on an unbound recursive variable")
        }
        Type::Callable(callable) => complete(callable),
        Type::Dynamic(_) => {
            admission.dependency(CallableConversionOperation::DynamicSignature)?;
            let signature = if request.ty.is_unknown() && request.unknown_is_recovery {
                Signature::recursion_recovery()
            } else {
                Signature::dynamic(request.ty)
            };
            complete(CallableType::function_like(db, signature))
        }
        Type::Divergent(_) => {
            admission.dependency(CallableConversionOperation::DynamicSignature)?;
            complete(CallableType::function_like(
                db,
                Signature::dynamic(request.ty),
            ))
        }
        Type::Recursive(recursive) => {
            admission.dependency(CallableConversionOperation::RecursiveTypeUnfold)?;
            let Some(unfolded) = recursive.unfold(db, env).into_unfolded() else {
                return Ok(ConversionStep::Complete(None));
            };
            request.convert(
                unfolded,
                ConversionContinuation::Transform(ConversionTransform::Identity),
            )
        }
        Type::FunctionLiteral(function)
            if is_recursive_reference(db, request, function, admission)? =>
        {
            admission.dependency(CallableConversionOperation::DynamicSignature)?;
            complete(CallableType::bottom(db))
        }
        Type::FunctionLiteral(function) => ConversionStep::Function(PendingFunction { function }),
        Type::BoundMethod(method) => {
            admission.dependency(CallableConversionOperation::BoundMethod)?;
            if request.recursive_definition.is_some()
                && method
                    .function(db)
                    .map(|function| is_recursive_reference(db, request, function, admission))
                    .transpose()?
                    .unwrap_or(false)
            {
                admission.dependency(CallableConversionOperation::DynamicSignature)?;
                complete(CallableType::bottom(db))
            } else if has_recursion_guard {
                request.convert(
                    method.func(db),
                    ConversionContinuation::Transform(ConversionTransform::BindReceiver(method)),
                )
            } else {
                ConversionStep::CachedBoundMethod(method)
            }
        }
        Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
            ConversionStep::CallMember(PendingCallMember { request })
        }
        Type::ClassLiteral(class_literal) => {
            admission.dependency(CallableConversionOperation::ClassIdentity)?;
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
        Type::NewTypeInstance(newtype) => {
            admission.dependency(CallableConversionOperation::NewTypeBase)?;
            request.convert(
                newtype.concrete_base_type(db),
                ConversionContinuation::Transform(ConversionTransform::Identity),
            )
        }
        Type::SubclassOf(subclass) if request.policy == UpcastPolicy::Sound => {
            ConversionStep::SubclassInstance(subclass)
        }
        // TODO: This is unsound so in future we can consider an opt-in option to disable it.
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Class(class) => ConversionStep::Constructor {
                class,
                receiver: Type::from(class),
            },
            SubclassOfInner::Protocol(protocol) => {
                admission.dependency(CallableConversionOperation::ProtocolConstructor)?;
                let Some(origin) = protocol.class_origin(db) else {
                    return Ok(ConversionStep::Complete(None));
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
                admission.dependency(CallableConversionOperation::TypeVarBoundOrConstraints)?;
                match typevar.require_bound_or_constraints(db, env) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => request.convert(
                        bound.constructor_for_typevar_bound(db, env),
                        ConversionContinuation::Transform(ConversionTransform::ReplaceReturn(
                            Type::TypeVar(typevar),
                        )),
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
            SubclassOfInner::Dynamic(_) => {
                admission.dependency(CallableConversionOperation::DynamicSignature)?;
                complete(CallableType::single(
                    db,
                    Signature::new(Parameters::unknown(), Type::from(subclass)),
                ))
            }
        },
        Type::Union(union) => {
            admission.dependency(CallableConversionOperation::RuntimeUnion)?;
            ConversionStep::RuntimeUnion(RuntimeUnionConversion {
                request,
                elements: union.elements(db),
            })
        }
        Type::LiteralValue(literal) => match literal.kind() {
            LiteralValueTypeKind::Enum(enum_literal) => {
                admission.dependency(CallableConversionOperation::EnumInstance)?;
                request.convert(
                    enum_literal.enum_class_instance(db, env),
                    ConversionContinuation::Transform(ConversionTransform::Identity),
                )
            }
            _ => ConversionStep::Complete(None),
        },
        Type::TypeAlias(alias) => {
            admission.dependency(CallableConversionOperation::TypeAliasValue)?;
            request.convert(
                alias.value_type(db),
                ConversionContinuation::Transform(ConversionTransform::Identity),
            )
        }
        Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => {
            admission.dependency(CallableConversionOperation::Continuation)?;
            request.convert(
                callable.inner(db),
                ConversionContinuation::Transform(ConversionTransform::Regularize),
            )
        }
        Type::KnownBoundMethod(method) => {
            admission.dependency(CallableConversionOperation::KnownBoundMethod)?;
            ConversionStep::Complete(method.callables(db, env))
        }
        Type::WrapperDescriptor(wrapper) => {
            admission.dependency(CallableConversionOperation::WrapperSignature)?;
            complete(CallableType::new(
                db,
                CallableSignature::from_overloads(wrapper.signatures(db, env)),
                CallableTypeKind::Regular,
            ))
        }
        Type::KnownInstance(KnownInstanceType::NewType(newtype)) => {
            admission.dependency(CallableConversionOperation::NewTypeSignature)?;
            complete(CallableType::single(
                db,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(None)
                        .with_annotated_type(newtype.base(db).instance_type(db, env))]),
                    Type::NewTypeInstance(newtype),
                ),
            ))
        }
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
        ) => {
            admission.dependency(CallableConversionOperation::PartialSignature)?;
            complete(partial.partial(db))
        }
        Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
            admission.dependency(CallableConversionOperation::MethodWrapper)?;
            match wrapper.kind(db) {
                MethodWrapperKind::Staticmethod => request.convert(
                    wrapper.wrapped(db),
                    ConversionContinuation::Transform(ConversionTransform::Identity),
                ),
                MethodWrapperKind::Classmethod => ConversionStep::Complete(None),
            }
        }
        Type::Intersection(intersection) => {
            admission.dependency(CallableConversionOperation::IntersectionAlternatives)?;
            let Some(alternatives) = intersection.finite_alternative_union(db, env) else {
                return Ok(ConversionStep::Complete(None));
            };
            request.convert(
                alternatives,
                ConversionContinuation::Transform(ConversionTransform::Identity),
            )
        }
        Type::EnumComplement(complement) => {
            admission.dependency(CallableConversionOperation::EnumComplement)?;
            request.convert(
                complement.remaining_literal_union(db, env),
                ConversionContinuation::Transform(ConversionTransform::Identity),
            )
        }
        // TODO
        Type::DataclassDecorator(_)
        | Type::ModuleLiteral(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::SlotDescriptor(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_) => ConversionStep::Complete(None),
    })
}

shared_semantic_family! {
    /// Builds the sound callable upper bound while retaining its top parameters across instance conversion.
    #[synchronous(SynchronousSubclassCallableEffects)]
    pub(in crate::types) trait SubclassCallableEffects<'db> {
        type Error;
        #[operation(child)]
        async fn top_parameters(&self) -> Result<Parameters<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn function_like(&self, parameters: Parameters<'db>, return_type: Type<'db>) -> Result<CallableTypes<'db>, Self::Error>;
    }

    /// Converts `type[T]` soundly without assuming that subclasses preserve T's constructor signature.
    #[synchronous(subclass_callable_sync)]
    #[capabilities(effects = SubclassCallableEffects)]
    #[passive_values()]
    pub(in crate::types) async fn subclass_callable_with<'db, E: SubclassCallableEffects<'db>>(
        subclass: SubclassOfType<'db>, effects: &E,
    ) -> Result<CallableTypes<'db>, E::Error> {
        let parameters = effects.top_parameters().await?;
        let instance = effects.subclass_instance(subclass).await?;
        effects.function_like(parameters, instance).await
    }
}

/// Supplies the ordinary constructors to the same sequence used by controlled conversion.
struct OrdinarySubclassCallable<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousSubclassCallableEffects<'db> for OrdinarySubclassCallable<'_, 'db> {
    type Error = Infallible;

    fn top_parameters(&self) -> Result<Parameters<'db>, Infallible> {
        Ok(Parameters::top())
    }

    fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(subclass.to_instance(self.db, self.env))
    }

    fn function_like(
        &self,
        parameters: Parameters<'db>,
        return_type: Type<'db>,
    ) -> Result<CallableTypes<'db>, Infallible> {
        Ok(CallableTypes::one(CallableType::function_like(
            self.db,
            Signature::new(parameters, return_type),
        )))
    }
}

/// Runs the shared sound subclass conversion in the ordinary evaluator.
pub(super) fn subclass_callable<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    subclass: SubclassOfType<'db>,
) -> CallableTypes<'db> {
    match subclass_callable_sync(subclass, &OrdinarySubclassCallable { db, env }) {
        Ok(callables) => callables,
        Err(never) => match never {},
    }
}

fn is_recursive_reference<'db, A: ConversionAdmission>(
    db: &'db dyn Db,
    request: CallableConversionRequest<'db>,
    function: FunctionType<'db>,
    admission: &A,
) -> Result<bool, A::Error> {
    if request.recursive_definition.is_none() {
        return Ok(false);
    }
    admission.dependency(CallableConversionOperation::RecursiveReference)?;
    Ok(request.is_recursive_reference(db, function))
}

pub(in crate::types) struct PendingFunction<'db> {
    pub(in crate::types) function: FunctionType<'db>,
}

impl<'db> PendingFunction<'db> {
    pub(in crate::types) fn resume(self, callable: CallableType<'db>) -> ConversionStep<'db> {
        ConversionStep::Complete(Some(CallableTypes::one(callable)))
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

pub(in crate::types) struct PendingConversion<'db> {
    pub(super) request: CallableConversionRequest<'db>,
    pub(super) origin: Option<DescriptorOrigin<'db>>,
    continuation: ConversionContinuation<'db>,
}

enum ConversionContinuation<'db> {
    Transform(ConversionTransform<'db>),
    Alternatives(ConversionAlternatives<'db>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConversionTransform<'db> {
    Identity,
    Regularize,
    BindReceiver(BoundMethodType<'db>),
    ReplaceReturn(Type<'db>),
}

impl<'db> ConversionTransform<'db> {
    pub(crate) fn apply(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callables: CallableTypes<'db>,
    ) -> CallableTypes<'db> {
        match self {
            Self::Identity => callables,
            // The callable instance itself doesn't inherit the descriptor behavior of
            // its `__call__` method.
            Self::Regularize => callables.map(|callable| callable.into_regular(db)),
            Self::BindReceiver(method) => callables.map(|callable| {
                callable.bind_self(
                    db,
                    env,
                    method.signature_receiver(db),
                    method.typing_self_type(db),
                )
            }),
            Self::ReplaceReturn(return_type) => {
                callables.map(|callable| with_return_type(db, callable, return_type))
            }
        }
    }
}

impl<'db> PendingConversion<'db> {
    #[cfg(test)]
    pub(super) fn transform(&self) -> Option<ConversionTransform<'db>> {
        match self.continuation {
            ConversionContinuation::Transform(transform) => Some(transform),
            ConversionContinuation::Alternatives(_) => None,
        }
    }

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
            ConversionContinuation::Transform(transform) => transform.apply(db, env, callables),
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

/// Every runtime alternative must be callable before the union has a complete conversion.
pub(in crate::types) struct RuntimeUnionConversion<'db> {
    request: CallableConversionRequest<'db>,
    elements: &'db [Type<'db>],
}

impl<'db> RuntimeUnionConversion<'db> {
    #[cfg(test)]
    pub(super) fn requests(
        &self,
    ) -> impl ExactSizeIterator<Item = CallableConversionRequest<'db>> + '_ {
        self.elements.iter().map(|ty| CallableConversionRequest {
            ty: *ty,
            ..self.request
        })
    }

    pub(super) fn sequential(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> ConversionStep<'db> {
        ConversionAlternatives {
            request: self.request,
            remaining: self.elements,
            return_type: None,
            callables: SmallVec::new(),
        }
        .advance(db, env)
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

pub(in crate::types) struct PendingCallMember<'db> {
    request: CallableConversionRequest<'db>,
}

pub(in crate::types) struct CallMemberFacts;

shared_semantic_family! {
    #[synchronous(SynchronousCallMemberEffects)]
    pub(in crate::types) trait CallMemberEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn lookup(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(source)]
        async fn fallback_member(&self, result: MemberLookupResult<'db>) -> Result<ResolvedMember<'db>, Self::Error>;
        #[operation(source)]
        async fn place(&self, member: ResolvedMember<'db>) -> Result<Place<'db>, Self::Error>;
        #[operation(source)]
        async fn origin(&self, member: ResolvedMember<'db>) -> Result<DescriptorOrigin<'db>, Self::Error>;
    }

    #[finite_capability]
    impl CallMemberFacts {
        fn receiver<'db>(&self, pending: &PendingCallMember<'db>) -> Type<'db> {
            pending.request.ty
        }

        fn defined_type<'db>(&self, place: Place<'db>) -> Option<Type<'db>> {
            match place {
                Place::Defined(place) if place.is_definitely_defined() => Some(place.ty),
                _ => None,
            }
        }

        fn resume<'db>(&self, pending: PendingCallMember<'db>, ty: Type<'db>, origin: DescriptorOrigin<'db>) -> ConversionStep<'db> {
            ConversionStep::Convert(PendingConversion {
                request: CallableConversionRequest {
                    ty,
                    unknown_is_recovery: origin.return_contains_recursive_recovery,
                    ..pending.request
                },
                origin: Some(origin),
                continuation: ConversionContinuation::Transform(ConversionTransform::Regularize),
            })
        }
    }

    /// Looks up `__call__` and resumes callable conversion only for a definitely defined member.
    /// Lookup errors use their fallback member, retaining its descriptor origin for conversion.
    #[synchronous(call_member_sync)]
    #[capabilities(effects = CallMemberEffects, facts = CallMemberFacts)]
    #[passive_values(ConversionStep::Complete)]
    pub(in crate::types) async fn call_member_with<'db, E: CallMemberEffects<'db>>(
        pending: PendingCallMember<'db>,
        facts: CallMemberFacts,
        effects: &E,
    ) -> Result<ConversionStep<'db>, E::Error> {
        effects.checkpoint().await?;
        let result = effects.lookup(facts.receiver(&pending)).await?;
        let member = effects.fallback_member(result).await?;
        let place = effects.place(member).await?;
        let Some(ty) = facts.defined_type(place) else {
            return Ok(ConversionStep::Complete(None));
        };
        let origin = effects.origin(member).await?;
        Ok(facts.resume(pending, ty, origin))
    }
}

struct OrdinaryCallMemberEffects<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    guard: Option<&'a CallableRecursionGuard<'db>>,
}

impl<'db> SynchronousCallMemberEffects<'db> for OrdinaryCallMemberEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn lookup(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(ty.member_lookup_with_recursion_guard(
            self.db,
            self.env,
            "__call__",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            None,
            self.guard,
        ))
    }

    fn fallback_member(
        &self,
        result: MemberLookupResult<'db>,
    ) -> Result<ResolvedMember<'db>, Infallible> {
        Ok(result.unwrap_or_else(|error| error.fallback_member(self.db)))
    }

    fn place(&self, member: ResolvedMember<'db>) -> Result<Place<'db>, Infallible> {
        Ok(member.member(self.db).place)
    }

    fn origin(&self, member: ResolvedMember<'db>) -> Result<DescriptorOrigin<'db>, Infallible> {
        Ok(member.descriptor_origin(self.db))
    }
}

impl<'db> PendingCallMember<'db> {
    pub(super) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> ConversionStep<'db> {
        match call_member_sync(
            self,
            CallMemberFacts,
            &OrdinaryCallMemberEffects {
                db,
                env,
                guard: recursion_guard,
            },
        ) {
            Ok(step) => step,
            Err(never) => match never {},
        }
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
