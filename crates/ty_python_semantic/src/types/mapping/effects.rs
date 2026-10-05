//! Dependencies and pre-work reservations for shared type mapping.

use std::convert::Infallible;
use std::future::Future;

use super::{
    LegacyTypeMappingContinuation, MappingStart, TypeMappingContinuation,
    TypeVarMappingContinuation,
};
use crate::types::cyclic::{
    InlineTypeTransformationControl, TypeTransformationScope, TypeTransformerVisit,
};
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingTag, ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, SelfBinding,
    Type, TypeContext, TypeMapping, TypeVarVariance,
};
use crate::types::{
    BoundMethodType, CallableType, ClassLiteral, DivergentType, EnumComplementType, FunctionType,
    InternedType, IntersectionType, KnownBoundMethodType, KnownInstanceType, KnownUnion,
    MaterializationKind, NewType, NominalInstanceType, PromotionKind, PromotionMode,
    PropertyInstanceType, ProtocolInstanceType, RecursiveType, RecursiveVar, SlotDescriptorType,
    SubclassOfType, TypeAliasType, TypeFormType, TypeGuardType, TypeIsType, TypedDictType,
    UnionType,
};
use crate::{Db, ProgramEnvironment};

/// Amounts describe immediate stored payloads, never the transitive size of a type handle.
/// Providers apply their cost policy with checked arithmetic before granting a reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MappingWork {
    RootAdmission,
    TypeDispatch,
    TypeVarLookup,
    NominalPromotionClass,
    VarianceLookup,
    ScalarFallbackLookup,
    ArgumentAdvance,
    ChildRequest,
    ArgumentCapacity { width: usize },
    ArgumentPrefixCopy { len: usize },
    ArgumentAppend,
    SpecializationPayload { width: usize },
    GenericAliasIntern,
    ExplicitAnyIntern,
    WrapperIntern,
    ResultPublication,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum MappingOperation {
    MappingMode,
    FunctionParamSpecPrelude,
    KnownInstance,
    Recursive,
    Function,
    BoundMethod,
    Promotion,
    NewType,
    Protocol,
    KnownBoundMethod,
    Callable,
    TypedDict,
    Property,
    SlotDescriptor,
    Union,
    Intersection,
    EnumComplement,
    TypeIs,
    TypeGuard,
    TypeForm,
    TypeAlias,
    AnnotationContext,
    MaterializationOrPolarity,
    Tuple,
    ParamSpec,
    RetainedSelf,
    SubclassTypeVar,
    SignatureReceiverConstraints,
    SignatureStarredExpansion,
    GenericContextSelfRemoval,
}

pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

pub(crate) trait MappingFacts<'db>: sealed::Sealed {
    type Error;

    fn variance(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Self::Error>;

    fn scalar_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(crate) type MappingTransformationScope<'a, 'db> =
    TypeTransformationScope<'a, 'db, ApplyTypeMappingTag>;

pub(crate) trait MappingStartEffects<'db>:
    MappingFacts<'db> + SharedMappingStartEffects<'db, Failure = <Self as MappingFacts<'db>>::Error>
{
    /// All reads and traversal belonging to the operation stay inside the thunk. A provider
    /// that cannot implement that operation rejects it without evaluating any part of it.
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        thunk: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
}

pub(crate) trait MappingEffects<'db>:
    MappingStartEffects<'db> + SharedMappingEffects<'db, Failure = <Self as MappingFacts<'db>>::Error>
{
    fn should_bind_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;
}

pub(crate) trait SynchronousMappingStartEffects<'db>:
    MappingFacts<'db>
    + SynchronousSharedMappingStartEffects<'db, Failure = <Self as MappingFacts<'db>>::Error>
{
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        thunk: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
}

pub(crate) trait SynchronousMappingEffects<'db>:
    SynchronousMappingStartEffects<'db>
    + SynchronousSharedMappingEffects<'db, Failure = <Self as MappingFacts<'db>>::Error>
{
    fn should_bind_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSharedMappingStartEffects)]
    pub(crate) trait SharedMappingStartEffects<'db> {
        type Failure;
        #[operation(checkpoint)]
        async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Failure>;
        #[operation(local)]
        async fn admit_mode(&self, mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure>;
        #[operation(child)]
        async fn expand_paramspecs(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Option<Type<'db>>, Self::Failure>;
        #[operation(local)]
        async fn nominal_known_class(&self, db: &'db dyn Db, instance: NominalInstanceType<'db>) -> Result<Option<KnownClass>, Self::Failure>;
        #[operation(child)]
        async fn start_typevar(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>, mapping: &TypeMapping<'_, 'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, Self::Failure>;
        #[operation(local)]
        async fn begin_transformation<'v>(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, visitor: &'v ApplyTypeMappingVisitor<'_, 'db>) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'v, 'db>>, Self::Failure>;
        #[operation(child)]
        async fn legacy_leaf(&self, db: &'db dyn Db, ty: Type<'db>, leaf: NativeMappingLeaf<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
    }
    #[synchronous(SynchronousSharedMappingEffects)]
    pub(crate) trait SharedMappingEffects<'db>: SharedMappingStartEffects<'db> {
        #[operation(child)]
        async fn map_union(&self, db: &'db dyn Db, union: UnionType<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(child)]
        async fn map_intersection(&self, db: &'db dyn Db, intersection: IntersectionType<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(child)]
        async fn map_tuple(&self, db: &'db dyn Db, tuple: TupleType<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(local)]
        async fn typeform_argument(&self, db: &'db dyn Db, typeform: TypeFormType<'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(source)]
        async fn intern_typeform(&self, db: &'db dyn Db, argument: Type<'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(local)]
        async fn finish_transformation(&self, scope: MappingTransformationScope<'_, 'db>, result: Type<'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
        #[operation(child)]
        async fn resume_legacy(&self, db: &'db dyn Db, continuation: LegacyTypeMappingContinuation<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Failure>;
    }
    #[synchronous(SynchronousMaterializationEffects)]
    pub(crate) trait MaterializationEffects<'db> {
        type Failure;
        #[operation(local)]
        async fn nominal_is_generic(&self, db: &'db dyn Db, instance: NominalInstanceType<'db>) -> Result<bool, Self::Failure>;
        #[operation(child)]
        async fn cached_materialization(&self, db: &'db dyn Db, ty: Type<'db>, kind: MaterializationKind) -> Result<Type<'db>, Self::Failure>;
    }
    #[synchronous(SynchronousMappingDriverEffects)]
    pub(crate) trait MappingDriverEffects<'v, 'db: 'v> {
        type Failure;
        #[operation(child)]
        async fn start(&self) -> Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, Self::Failure>;
        #[operation(child)]
        async fn resume(&self, continuation: TypeMappingContinuation<'v, 'db>) -> Result<Type<'db>, Self::Failure>;
    }
    #[finite_capability]
    impl MappingDispatchFacts {
        fn is_bound_self<'db>(&self, ty: Type<'db>, binding: &SelfBinding<'db>) -> bool { ty == binding.self_type() }
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn materialized(&self, divergent: DivergentType, kind: &MaterializationKind) -> DivergentType { divergent.materialized(*kind) }
        fn exact_tuple<'db>(&self, instance: NominalInstanceType<'db>) -> Option<TupleType<'db>> { instance.exact_tuple() }
    }
    #[synchronous(materialization_sync)]
    #[capabilities(effects = MaterializationEffects, facts = MappingDispatchFacts)]
    #[passive_values(Type::Never, Type::Divergent)]
    pub(crate) async fn materialization_with<'db, E: MaterializationEffects<'db>>(
        db: &'db dyn Db, ty: Type<'db>, kind: MaterializationKind,
        effects: &E, facts: MappingDispatchFacts,
    ) -> Result<Type<'db>, E::Failure> {
        let result = match ty {
            Type::Dynamic(_) => match kind {
                MaterializationKind::Top => facts.object(),
                MaterializationKind::Bottom => Type::Never,
            },
            Type::Divergent(divergent) => Type::Divergent(facts.materialized(divergent, &kind)),
            Type::Never | Type::AlwaysTruthy | Type::AlwaysFalsy | Type::ClassLiteral(_)
            | Type::LiteralValue(_) | Type::ModuleLiteral(_) | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_) | Type::DataclassTransformer(_) | Type::BoundSuper(_)
            | Type::SpecialForm(_) => ty,
            Type::NominalInstance(instance) if !effects.nominal_is_generic(db, instance).await? => ty,
            _ => effects.cached_materialization(db, ty, kind).await?,
        };
        Ok(result)
    }
    #[synchronous(apply_type_mapping_sync)]
    #[capabilities(effects = MappingDriverEffects)]
    #[passive_values()]
    pub(crate) async fn apply_type_mapping_with<'v, 'db: 'v, E: MappingDriverEffects<'v, 'db>>(effects: &E) -> Result<Type<'db>, E::Failure> {
        match effects.start().await? {
            MappingStart::Complete(result) => Ok(result),
            MappingStart::Continue(continuation) => effects.resume(continuation).await,
        }
    }
    #[synchronous(complete_mapping_sync)]
    #[capabilities(effects = SharedMappingStartEffects)]
    #[passive_values(MappingWork::ResultPublication)]
    pub(crate) async fn complete_mapping_with<'db, E: SharedMappingStartEffects<'db>>(ty: Type<'db>, effects: &E) -> Result<Type<'db>, E::Failure> {
        effects.checkpoint(MappingWork::ResultPublication).await?;
        Ok(ty)
    }
    #[synchronous(resume_type_mapping_sync)]
    #[capabilities(effects = SharedMappingEffects)]
    #[passive_values(MappingWork::ChildRequest, MappingWork::WrapperIntern, MappingWork::ResultPublication, Type::FunctionLiteral, Type::Callable, NativeMappingLeaf::Function, NativeMappingLeaf::Callable, LegacyTypeMappingContinuation::Promotion)]
    pub(crate) async fn resume_type_mapping_with<'db, E: SharedMappingEffects<'db>>(
        db: &'db dyn Db, continuation: TypeMappingContinuation<'_, 'db>, mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E,
    ) -> Result<Type<'db>, E::Failure> {
        let mapped = match continuation {
            TypeMappingContinuation::Union(union) => effects.map_union(db, union, mapping, tcx, visitor).await?,
            TypeMappingContinuation::Intersection(intersection) => effects.map_intersection(db, intersection, mapping, tcx, visitor).await?,
            TypeMappingContinuation::Tuple(tuple) => effects.map_tuple(db, tuple, mapping, tcx, visitor).await?,
            TypeMappingContinuation::Legacy(legacy) => effects.resume_legacy(db, legacy, mapping, tcx, visitor).await?,
            TypeMappingContinuation::Function { function, scope } => {
                let mapped = effects.legacy_leaf(db, Type::FunctionLiteral(function), NativeMappingLeaf::Function(function), mapping, tcx, visitor).await?;
                let result = match mapping {
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                        effects.resume_legacy(db, LegacyTypeMappingContinuation::Promotion(mapped), mapping, tcx, visitor).await?
                    }
                    _ => mapped,
                };
                effects.finish_transformation(scope, result).await?
            }
            TypeMappingContinuation::Callable { callable, scope } => {
                let mapped = effects.legacy_leaf(db, Type::Callable(callable), NativeMappingLeaf::Callable(callable), mapping, tcx, visitor).await?;
                effects.finish_transformation(scope, mapped).await?
            }
            TypeMappingContinuation::TypeForm { typeform, scope } => {
                effects.checkpoint(MappingWork::ChildRequest).await?;
                let argument = effects.typeform_argument(db, typeform).await?;
                let mapped = effects.map_type(db, argument, mapping, tcx, visitor).await?;
                effects.checkpoint(MappingWork::WrapperIntern).await?;
                let result = effects.intern_typeform(db, mapped).await?;
                effects.finish_transformation(scope, result).await?
            }
        };
        effects.checkpoint(MappingWork::ResultPublication).await?;
        Ok(mapped)
    }
    #[synchronous(start_type_mapping_sync)]
    #[capabilities(effects = SharedMappingStartEffects, facts = MappingDispatchFacts)]
    #[passive_values(MappingWork::TypeDispatch, MappingStart::Complete, TypeMappingContinuation::Tuple, NativeMappingLeaf::ClassPromotion, MappingStart::Continue, TypeMappingContinuation::Legacy, LegacyTypeMappingContinuation::TypeVar, NativeMappingLeaf::KnownInstance, NativeMappingLeaf::Recursive, NativeMappingLeaf::RecursiveVar, NativeMappingLeaf::Function, NativeMappingLeaf::BoundMethod, MappingWork::NominalPromotionClass, NativeMappingLeaf::Complex, NativeMappingLeaf::Float, LegacyTypeMappingContinuation::Nominal, NativeMappingLeaf::SingletonNominal, NativeMappingLeaf::NewType, NativeMappingLeaf::Protocol, NativeMappingLeaf::FunctionGet, NativeMappingLeaf::CallableGet, NativeMappingLeaf::MethodGet, NativeMappingLeaf::PropertyGet, NativeMappingLeaf::PropertySet, NativeMappingLeaf::PropertyDelete, NativeMappingLeaf::Callable, LegacyTypeMappingContinuation::GenericAlias, NativeMappingLeaf::TypedDict, LegacyTypeMappingContinuation::SubclassOf, NativeMappingLeaf::Property, NativeMappingLeaf::SlotDescriptor, TypeMappingContinuation::Union, TypeMappingContinuation::Intersection, NativeMappingLeaf::EnumComplement, NativeMappingLeaf::TypeIs, NativeMappingLeaf::TypeGuard, TypeMappingContinuation::TypeForm, TypeMappingContinuation::Function, TypeMappingContinuation::Callable, NativeMappingLeaf::TypeAlias, LegacyTypeMappingContinuation::Promotion, Type::Never, Type::Divergent, MappingWork::ResultPublication)]
    pub(crate) async fn start_type_mapping_with<'a, 'v, 'db, E: SharedMappingStartEffects<'db>>(
        db: &'db dyn Db, ty: Type<'db>, type_mapping: &TypeMapping<'a, 'db>, tcx: TypeContext<'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>, effects: &E, facts: MappingDispatchFacts,
    ) -> Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, E::Failure> {

        effects.checkpoint(MappingWork::TypeDispatch).await?;
        if !matches!(
            type_mapping,
            TypeMapping::ApplySpecialization(_)
                | TypeMapping::Promote(_, PromotionKind::Regular)
                | TypeMapping::BindSelf(_)
        ) {
            effects.admit_mode(type_mapping).await?;
        }

        // If we are binding `typing.Self`, and this type is what we are binding `Self` to, return
        // early. This is not just an optimization, it also prevents us from infinitely expanding
        // the type, if it's something that can contain a `Self` reference.
        match type_mapping {
            TypeMapping::BindSelf(binding) if facts.is_bound_self(ty, binding) => {
                return Ok(MappingStart::Complete(ty));
            }
            _ => {}
        }

        // Recursive singleton promotion only recurses into `NominalInstance` types (tuples
        // and specialized generics). For all other types, return early.
        if matches!(
            type_mapping,
            TypeMapping::Promote(_, PromotionKind::SingletonsOnly)
        ) && !matches!(ty, Type::NominalInstance(_))
        {
            return Ok(MappingStart::Complete(ty));
        }

        if let Type::ClassLiteral(class) = ty
            && matches!(
                type_mapping,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::ClassLiteralsOnly)
            )
        {
            let result = effects.legacy_leaf(db, ty, NativeMappingLeaf::ClassPromotion(class), type_mapping, tcx, visitor).await?;
            return Ok(MappingStart::Complete(result));
        }

        if matches!(type_mapping, TypeMapping::ApplySpecialization(_) | TypeMapping::ApplySpecializationWithMaterialization { .. }) && matches!(
            ty,
            Type::FunctionLiteral(_)
                | Type::BoundMethod(_)
                | Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(_))
                | Type::Callable(_)
        ) && let Some(expanded) = effects.expand_paramspecs(db, ty, type_mapping, tcx, visitor).await?
        {
            return Ok(MappingStart::Complete(expanded));
        }

        let mapped = match ty {
            Type::TypeVar(bound_typevar) => {
                match effects.start_typevar(db, bound_typevar, type_mapping, visitor)
                    .await?
                {
                    MappingStart::Complete(mapped) => mapped,
                    MappingStart::Continue(continuation) => {
                        return Ok(MappingStart::Continue(TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::TypeVar(
                            continuation,
                        ))));
                    }
                }
            }
            Type::KnownInstance(known_instance) => effects.legacy_leaf(db, ty, NativeMappingLeaf::KnownInstance(known_instance), type_mapping, tcx, visitor).await?,

            Type::Recursive(recursive) => effects.legacy_leaf(db, ty, NativeMappingLeaf::Recursive(recursive), type_mapping, tcx, visitor).await?,
            Type::RecursiveVar(reference) => effects.legacy_leaf(db, ty, NativeMappingLeaf::RecursiveVar(reference), type_mapping, tcx, visitor).await?,

            Type::FunctionLiteral(function) => {
                match effects.begin_transformation(db, ty, type_mapping, visitor).await? {
                    TypeTransformerVisit::Ready(result) => result,
                    TypeTransformerVisit::Pending(scope) => {
                        return Ok(MappingStart::Continue(TypeMappingContinuation::Function { function, scope }));
                    }
                }
            }

            Type::BoundMethod(method) => effects.legacy_leaf(db, ty, NativeMappingLeaf::BoundMethod(method), type_mapping, tcx, visitor).await?,

            Type::NominalInstance(instance)
                if matches!(
                    type_mapping,
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular)
                ) =>
            {
                effects
                    .checkpoint(MappingWork::NominalPromotionClass)
                    .await?;
                match effects.nominal_known_class(db, instance).await? {
                    Some(KnownClass::Complex) => effects.legacy_leaf(db, ty, NativeMappingLeaf::Complex, type_mapping, tcx, visitor).await?,
                    Some(KnownClass::Float) => effects.legacy_leaf(db, ty, NativeMappingLeaf::Float, type_mapping, tcx, visitor).await?,
                    _ => {
                        return Ok(MappingStart::Continue(TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::Nominal(
                            instance,
                        ))));
                    }
                }
            }

            Type::NominalInstance(instance)
                if matches!(
                    type_mapping,
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly)
                ) =>
            {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::SingletonNominal(instance), type_mapping, tcx, visitor).await?
            }

            Type::NominalInstance(instance) => {
                if let Some(tuple) = facts.exact_tuple(instance) {
                    return Ok(MappingStart::Continue(TypeMappingContinuation::Tuple(tuple)));
                }
                return Ok(MappingStart::Continue(TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::Nominal(
                    instance,
                ))));
            }

            Type::NewTypeInstance(newtype) => effects.legacy_leaf(db, ty, NativeMappingLeaf::NewType(newtype), type_mapping, tcx, visitor).await?,

            Type::ProtocolInstance(instance) => {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::Protocol(instance), type_mapping, tcx, visitor).await?
            }

            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)) => {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::FunctionGet(function), type_mapping, tcx, visitor).await?
            }

            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::CallableGet(callable), type_mapping, tcx, visitor).await?
            }

            Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(method)) => effects.legacy_leaf(db, ty, NativeMappingLeaf::MethodGet(method), type_mapping, tcx, visitor).await?,

            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)) => effects.legacy_leaf(db, ty, NativeMappingLeaf::PropertyGet(property), type_mapping, tcx, visitor).await?,

            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)) => effects.legacy_leaf(db, ty, NativeMappingLeaf::PropertySet(property), type_mapping, tcx, visitor).await?,
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(property)) => effects.legacy_leaf(db, ty, NativeMappingLeaf::PropertyDelete(property), type_mapping, tcx, visitor).await?,

            Type::Callable(callable) => {
                match effects.begin_transformation(db, ty, type_mapping, visitor).await? {
                    TypeTransformerVisit::Ready(result) => result,
                    TypeTransformerVisit::Pending(scope) => {
                        return Ok(MappingStart::Continue(TypeMappingContinuation::Callable { callable, scope }));
                    }
                }
            }

            Type::GenericAlias(generic) => {
                return Ok(MappingStart::Continue(
                    TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::GenericAlias(generic)),
                ));
            }

            Type::TypedDict(typed_dict) => effects.legacy_leaf(db, ty, NativeMappingLeaf::TypedDict(typed_dict), type_mapping, tcx, visitor).await?,

            Type::SubclassOf(subclass_of) => {
                return Ok(MappingStart::Continue(TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::SubclassOf(
                    subclass_of,
                ))));
            }

            Type::PropertyInstance(property) => {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::Property(property), type_mapping, tcx, visitor).await?
            }

            Type::SlotDescriptor(descriptor) => {
                effects.legacy_leaf(db, ty, NativeMappingLeaf::SlotDescriptor(descriptor), type_mapping, tcx, visitor).await?
            }

            Type::Union(union) => return Ok(MappingStart::Continue(TypeMappingContinuation::Union(union))),
            Type::Intersection(intersection) => return Ok(MappingStart::Continue(TypeMappingContinuation::Intersection(intersection))),
            Type::EnumComplement(complement) => effects.legacy_leaf(db, ty, NativeMappingLeaf::EnumComplement(complement), type_mapping, tcx, visitor).await?,

            Type::TypeIs(type_is) => effects.legacy_leaf(db, ty, NativeMappingLeaf::TypeIs(type_is), type_mapping, tcx, visitor).await?,

            Type::TypeGuard(type_guard) => effects.legacy_leaf(db, ty, NativeMappingLeaf::TypeGuard(type_guard), type_mapping, tcx, visitor).await?,

            Type::TypeForm(typeform) => {
                match effects.begin_transformation(db, ty, type_mapping, visitor).await? {
                    TypeTransformerVisit::Ready(result) => result,
                    TypeTransformerVisit::Pending(scope) => {
                        return Ok(MappingStart::Continue(TypeMappingContinuation::TypeForm {
                            typeform,
                            scope,
                        }));
                    }
                }
            }

            Type::TypeAlias(alias) => effects.legacy_leaf(db, ty, NativeMappingLeaf::TypeAlias(alias), type_mapping, tcx, visitor).await?,

            Type::LiteralValue(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::ApplyRecursiveSubstitution(_)
                | TypeMapping::BindLegacyTypevars(_)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf { .. }
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Materialize(_)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_)
                | TypeMapping::Promote(PromotionMode::Off, _)
                | TypeMapping::Promote(
                    PromotionMode::On,
                    PromotionKind::ClassLiteralsOnly | PromotionKind::SingletonsOnly,
                ) => ty,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                    return Ok(MappingStart::Continue(TypeMappingContinuation::Legacy(LegacyTypeMappingContinuation::Promotion(
                        ty,
                    ))));
                }
            },

            Type::Dynamic(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::ApplyRecursiveSubstitution(_)
                | TypeMapping::BindLegacyTypevars(_)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf(..)
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Promote(..)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_) => ty,
                TypeMapping::Materialize(materialization_kind) => match materialization_kind {
                    MaterializationKind::Top => facts.object(),
                    MaterializationKind::Bottom => Type::Never,
                },
            },
            // `Divergent` is an internal cycle marker rather than a gradual type like `Any` or
            // `Unknown`. Preserve the marker across materialization, while recording whether this
            // occurrence should behave like the top (`object`) or bottom (`Never`) bound.
            Type::Divergent(divergent) => match type_mapping {
                TypeMapping::Materialize(materialization_kind) => {
                    Type::Divergent(facts.materialized(divergent, materialization_kind))
                }
                _ => ty,
            },

            Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => ty,

            // A non-generic class never needs to be specialized. A generic class is specialized
            // explicitly (via a subscript expression) or implicitly (via a call), and not because
            // some other generic context's specialization is applied to it.
            Type::ClassLiteral(_) => ty,
        };
        effects.checkpoint(MappingWork::ResultPublication).await?;
        Ok(MappingStart::Complete(mapped))
    }
}

pub(crate) struct MappingDispatchFacts;
pub(crate) struct MappingDriver<'a, 'm, 'v, 'env, 'db, E> {
    pub(crate) db: &'db dyn Db,
    pub(crate) ty: Type<'db>,
    pub(crate) mapping: &'a TypeMapping<'m, 'db>,
    pub(crate) tcx: TypeContext<'db>,
    pub(crate) visitor: &'v ApplyTypeMappingVisitor<'env, 'db>,
    pub(crate) effects: &'a E,
}
impl<'v, 'db, E: SharedMappingEffects<'db>> MappingDriverEffects<'v, 'db>
    for MappingDriver<'_, '_, 'v, '_, 'db, E>
{
    type Failure = E::Failure;
    fn start(
        &self,
    ) -> impl Future<
        Output = Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, E::Failure>,
    > {
        start_type_mapping_with(
            self.db,
            self.ty,
            self.mapping,
            self.tcx,
            self.visitor,
            self.effects,
            MappingDispatchFacts,
        )
    }
    fn resume(
        &self,
        continuation: TypeMappingContinuation<'v, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, E::Failure>> {
        resume_type_mapping_with(
            self.db,
            continuation,
            self.mapping,
            self.tcx,
            self.visitor,
            self.effects,
        )
    }
}
impl<'v, 'db, E: SynchronousSharedMappingEffects<'db>> SynchronousMappingDriverEffects<'v, 'db>
    for MappingDriver<'_, '_, 'v, '_, 'db, E>
{
    type Failure = E::Failure;
    fn start(
        &self,
    ) -> Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, E::Failure> {
        start_type_mapping_sync(
            self.db,
            self.ty,
            self.mapping,
            self.tcx,
            self.visitor,
            self.effects,
            MappingDispatchFacts,
        )
    }
    fn resume(
        &self,
        continuation: TypeMappingContinuation<'v, 'db>,
    ) -> Result<Type<'db>, E::Failure> {
        resume_type_mapping_sync(
            self.db,
            continuation,
            self.mapping,
            self.tcx,
            self.visitor,
            self.effects,
        )
    }
}

pub(crate) enum NativeMappingLeaf<'db> {
    ClassPromotion(ClassLiteral<'db>),
    KnownInstance(KnownInstanceType<'db>),
    Recursive(RecursiveType<'db>),
    RecursiveVar(RecursiveVar<'db>),
    Function(FunctionType<'db>),
    BoundMethod(BoundMethodType<'db>),
    Complex,
    Float,
    SingletonNominal(NominalInstanceType<'db>),
    NewType(NewType<'db>),
    Protocol(ProtocolInstanceType<'db>),
    FunctionGet(InternedType<'db>),
    CallableGet(InternedType<'db>),
    MethodGet(BoundMethodType<'db>),
    PropertyGet(PropertyInstanceType<'db>),
    PropertySet(PropertyInstanceType<'db>),
    PropertyDelete(PropertyInstanceType<'db>),
    Callable(CallableType<'db>),
    TypedDict(TypedDictType<'db>),
    Property(PropertyInstanceType<'db>),
    SlotDescriptor(SlotDescriptorType<'db>),
    EnumComplement(EnumComplementType<'db>),
    TypeIs(TypeIsType<'db>),
    TypeGuard(TypeGuardType<'db>),
    TypeAlias(TypeAliasType<'db>),
}
impl<'db> NativeMappingLeaf<'db> {
    pub(crate) fn operation(&self) -> MappingOperation {
        match self {
            Self::ClassPromotion(_) => MappingOperation::Promotion,
            Self::KnownInstance(_) => MappingOperation::KnownInstance,
            Self::Recursive(_) => MappingOperation::Recursive,
            Self::RecursiveVar(_) => MappingOperation::Recursive,
            Self::Function(_) => MappingOperation::Function,
            Self::BoundMethod(_) => MappingOperation::BoundMethod,
            Self::Complex => MappingOperation::Promotion,
            Self::Float => MappingOperation::Promotion,
            Self::SingletonNominal(_) => MappingOperation::Promotion,
            Self::NewType(_) => MappingOperation::NewType,
            Self::Protocol(_) => MappingOperation::Protocol,
            Self::FunctionGet(_) => MappingOperation::KnownBoundMethod,
            Self::CallableGet(_) => MappingOperation::KnownBoundMethod,
            Self::MethodGet(_) => MappingOperation::KnownBoundMethod,
            Self::PropertyGet(_) => MappingOperation::KnownBoundMethod,
            Self::PropertySet(_) => MappingOperation::KnownBoundMethod,
            Self::PropertyDelete(_) => MappingOperation::KnownBoundMethod,
            Self::Callable(_) => MappingOperation::Callable,
            Self::TypedDict(_) => MappingOperation::TypedDict,
            Self::Property(_) => MappingOperation::Property,
            Self::SlotDescriptor(_) => MappingOperation::SlotDescriptor,
            Self::EnumComplement(_) => MappingOperation::EnumComplement,
            Self::TypeIs(_) => MappingOperation::TypeIs,
            Self::TypeGuard(_) => MappingOperation::TypeGuard,
            Self::TypeAlias(_) => MappingOperation::TypeAlias,
        }
    }
    fn evaluate(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match self {
            Self::ClassPromotion(class) => {
                SubclassOfType::from(db, visitor.env, class.default_specialization(db))
            }
            Self::KnownInstance(known_instance) => {
                known_instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }
            Self::Recursive(recursive) => {
                recursive.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }
            Self::RecursiveVar(reference) => {
                reference.apply_type_mapping_impl(db, type_mapping, visitor)
            }
            Self::Function(function) => {
                Type::FunctionLiteral(function.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::BoundMethod(method) => {
                Type::BoundMethod(method.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::Complex => KnownUnion::Complex.to_type(db, visitor.env),
            Self::Float => KnownUnion::Float.to_type(db, visitor.env),
            Self::SingletonNominal(instance) => {
                if instance.is_singleton(db) {
                    ty.promote_singletons_impl(db, visitor.env)
                } else {
                    instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }
            }
            Self::NewType(newtype) => visitor.visit(db, ty, type_mapping, || {
                Type::NewTypeInstance(newtype.map_base_class_type(db, |class_type| {
                    class_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }))
            }),
            Self::Protocol(instance) => Type::ProtocolInstance(instance.apply_type_mapping_impl(
                db,
                type_mapping,
                tcx,
                visitor,
            )),
            Self::FunctionGet(function) => Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(InternedType::new(
                    db,
                    function
                        .inner(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )),
            ),
            Self::CallableGet(callable) => {
                let callable = match callable.inner(db) {
                    Type::FunctionLiteral(function) => Type::FunctionLiteral(
                        function.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                    ),
                    callable => callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                };
                Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(InternedType::new(
                    db, callable,
                )))
            }
            Self::MethodGet(method) => {
                Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(
                    method.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            Self::PropertyGet(property) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            Self::PropertySet(property) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            Self::PropertyDelete(property) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            Self::Callable(callable) => {
                Type::Callable(callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            },
            Self::TypedDict(typed_dict) => {
                Type::TypedDict(typed_dict.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::Property(property) => Type::PropertyInstance(property.apply_type_mapping_impl(
                db,
                type_mapping,
                tcx,
                visitor,
            )),
            Self::SlotDescriptor(descriptor) => Type::SlotDescriptor(SlotDescriptorType::new(
                db,
                descriptor
                    .value_type(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            )),
            Self::EnumComplement(complement) => {
                complement.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }
            Self::TypeIs(type_is) => visitor.visit(db, ty, type_mapping, || {
                type_is.with_type(
                    db,
                    type_is.type_argument(db).apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                )
            }),
            Self::TypeGuard(type_guard) => visitor.visit(db, ty, type_mapping, || {
                type_guard.with_type(
                    db,
                    type_guard.return_type(db).apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                )
            }),
            Self::TypeAlias(alias) => alias.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        }
    }
}
pub(crate) fn inline_mapping_result<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(crate) struct InlineMappingEffects;

impl sealed::Sealed for InlineMappingEffects {}

impl<'db> MappingFacts<'db> for InlineMappingEffects {
    type Error = Infallible;

    fn variance(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Infallible> {
        Ok(variable.variance(db))
    }

    fn scalar_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_instance(db, env))
    }
}

impl SynchronousMappingStartEffects<'_> for InlineMappingEffects {
    fn legacy<T>(
        &self,
        _operation: MappingOperation,
        thunk: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(thunk())
    }
}

impl<'db> SynchronousMappingEffects<'db> for InlineMappingEffects {
    fn should_bind_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(binding.should_bind(db, env, variable))
    }
}
impl<'db> SynchronousSharedMappingStartEffects<'db> for InlineMappingEffects {
    type Failure = Infallible;
    fn checkpoint(&self, _work: MappingWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn begin_transformation<'a>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Infallible> {
        visitor
            .transformer(mapping)
            .begin_visit_with(db, ty, &InlineTypeTransformationControl)
    }
    fn admit_mode(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure> {
        Ok(())
    }
    fn expand_paramspecs(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Failure> {
        Ok(ty.expand_union_paramspecs(db, mapping, tcx, visitor))
    }
    fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Failure> {
        Ok(instance.known_class(db))
    }
    fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, Self::Failure> {
        variable.mapping_start_sync(db, mapping, visitor, self)
    }
    fn legacy_leaf(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(leaf.evaluate(db, ty, mapping, tcx, visitor))
    }
}
impl<'db> SynchronousSharedMappingEffects<'db> for InlineMappingEffects {
    fn map_union(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(union.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
    fn map_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(intersection.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
    fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::tuple(
            tuple.apply_type_mapping_impl(db, mapping, tcx, visitor),
        ))
    }
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
    fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        scope.finish_with(result, &InlineTypeTransformationControl)
    }
    fn typeform_argument(
        &self,
        db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(typeform.type_argument(db))
    }
    fn intern_typeform(
        &self,
        db: &'db dyn Db,
        argument: Type<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(TypeFormType::from_type_expression(db, argument))
    }
    fn resume_legacy(
        &self,
        db: &'db dyn Db,
        continuation: LegacyTypeMappingContinuation<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        continuation.resume_mapping_sync(db, mapping, tcx, visitor, self)
    }
}
pub(crate) struct InlineMaterializationEffects<'env, 'db> {
    pub(crate) env: &'env ProgramEnvironment<'db>,
}
impl<'db> SynchronousMaterializationEffects<'db> for InlineMaterializationEffects<'_, 'db> {
    type Failure = Infallible;
    fn nominal_is_generic(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Failure> {
        Ok(instance.is_definition_generic(db))
    }
    fn cached_materialization(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(crate::types::cached_materialization(
            db,
            ty,
            self.env.program(db),
            kind,
        ))
    }
}

impl<'db> crate::types::mapping::specialization_start::SynchronousSpecializationStartEffects<'db>
    for InlineMappingEffects
{
    type Error = Infallible;
    fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
        SynchronousSharedMappingStartEffects::checkpoint(self, work)
    }
    fn nominal_is_definition_generic(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(instance.is_definition_generic(db))
    }
    fn typevar_is_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.is_paramspec(db))
    }
    fn lookup_typevar(
        &self,
        db: &'db dyn Db,
        specialization: crate::types::Specialization<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(specialization.get(db, variable))
    }
    fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: crate::types::Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    fn typevar_is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.typevar(db).is_self(db))
    }
}
