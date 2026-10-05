//! Source mapping keeps its root visitor alive across queued mapping descendants.

use std::borrow::Cow;
use std::slice;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use crate::{FxIndexMap, FxOrderSet};
use super::return_callables::{RetainedReturnCallables, RetainedReturnTypevars};

use salsa::execution_probe::{ExecutionWork, FieldReadProfile, FieldReturnMode, NativeValueQuote, RunError, RunResult, TaskEndpoint};

use super::effects::{
    MappingOperation, MappingTransformationScope, MappingWork, NativeMappingLeaf,
    SharedMappingEffects, SharedMappingStartEffects,
};
use super::{
    LegacyTypeMappingContinuation, MappingStart, MaterializationOperation, OwnedTypeMapping,
    TypeVarMappingContinuation,
};
use crate::types::class::mapping::{GenericAliasMappingEffects, map_generic_alias_with};
use crate::types::class_base::specialization::{
    ClassBaseMapping, ClassBaseMappingEffects, ClassBaseSpecializationWork, map_class_base_with,
};
use crate::types::constraints::control::GrowthPlan;
use crate::types::callable::CallableTypeKind;
use crate::types::function::{FunctionLiteral, OverloadLiteral, UpdatedFunctionSignatures};
use crate::types::signatures::{CallableSignature, Parameter, ParameterKind, Parameters, ParametersKind, Signature};
use crate::types::signatures::mapping::map_parameters_with;
use crate::types::cyclic::{
    TypeIdentity, TypeTransformationControl, TypeTransformationGrowth, TypeTransformationWork,
    TypeTransformer, TypeTransformerVisit,
};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::prefix::DefaultArgumentBuffer;
use crate::types::instance::ExplicitAnyInstanceClass;
use crate::types::known_instance::mapping::{KnownInstanceMappingEffects, map_known_instance_with};
use crate::types::known_instance::{FunctoolsPartialInstance, MethodWrapper, UnionTypeInstance};
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::set_theoretic::mapping::{
    SetMappingEffects, SetMappingFacts, map_intersection_with, map_union_with,
};
use crate::types::set_theoretic::{
    IntersectionBuilder, IntersectionType, RecursivelyDefined, UnionBuilder, UnionType,
};
use crate::types::tuple::buffer::{TupleBuffer, TupleBufferStorageEffects};
use crate::types::tuple::mapping::{
    MappedTupleVariable, TupleMappingEffects, TupleMappingFacts, map_tuple_spec_with,
    map_tuple_with,
};
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    ApplySpecialization, ApplyTypeMappingTag, ApplyTypeMappingVisitor, BindingContext,
    BoundTypeVarInstance, CallableType, ClassBase, ClassType, FunctionType, GenericAlias, GenericContext, InternedType,
    KnownClass, KnownInstanceType, KnownUnion, MaterializationKind, NominalInstanceType, Specialization,
    PromotionKind, SelfBinding, StaticClassLiteral, Type, TypeContext, TypeFormType, TypeMapping,
};
use crate::{Db, Program, ProgramEnvironment};

mod promotion;
mod function;
mod freshening;
#[cfg(test)]
pub(in crate::types) mod public_promotion_observations;
mod specialization;
mod typevar;

pub(in crate::types) use specialization::compose_specialization_with_retained;
#[cfg(test)]
pub(in crate::types) use specialization::observations as composition_observations;

/// Quotes generated borrowed and copied fields without treating representation width as work.
#[derive(Debug)]
pub(in crate::types) struct FixedMappingField;

impl<T> FieldReadProfile<T> for FixedMappingField {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call T,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        crate::types::local_transfer::local_with_fixed_transfers_at(endpoint, 3, 0, || {
            let requested_bytes = match mode {
                FieldReturnMode::Ref => size_of::<&T>(),
                FieldReturnMode::Copy => size_of::<T>(),
                FieldReturnMode::Clone | FieldReturnMode::Deref | FieldReturnMode::AsRef | FieldReturnMode::AsDeref => {
                    return Err(RunError::Contract("mapping field requires borrowed or copied conversion"));
                }
            };
            Ok(NativeValueQuote { work: 1, requested_bytes, cleanup_work: 0 })
        }).await?
    }
}

pub(in crate::types) trait MappingResourceAccess<'run, 'db: 'run>:
    Copy + 'run
{
    /// Maps one overload with its own retained visitor, without a temporary callable owner.
    async fn apply_signature_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Signature<'db>>;

    /// Retains an already admitted, completed insert-only map before any mapping child borrows it.
    async fn retain_return_typevars(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        values: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<RetainedReturnTypevars<'run, 'db>>;

    /// Retains completed insert-only callable replacements through the final return-type mapping and child drainage.
    async fn retain_return_callables(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        values: FxHashMap<CallableType<'db>, CallableType<'db>>,
    ) -> RunResult<RetainedReturnCallables<'run, 'db>>;

    /// Maps one callable's signatures with a fresh visitor, without interning a callable.
    async fn apply_callable_signature_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        signatures: &CallableSignature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<CallableSignature<'db>>;

    /// Maps a Self occurrence's present upper bound or ordered constraints using one new mapping visitor.
    async fn apply_retained_bounds<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        bounds: crate::types::TypeVarBoundOrConstraints<'db>,
        program: Program<'db>,
        specialization: Specialization<'db>,
        source: R,
    ) -> RunResult<crate::types::TypeVarBoundOrConstraints<'db>>;

    async fn default_arguments(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        len: usize,
    ) -> RunResult<DefaultArgumentBuffer<'run, 'db>>;

    async fn apply_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Type<'db>>;

    async fn apply_parameter_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        parameters: &Parameters<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Parameters<'db>>;

    async fn apply_class_base_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        base: ClassBase<'db>,
        program: Program<'db>,
        mapping: ClassBaseMapping<'db>,
        source: R,
    ) -> RunResult<ClassBase<'db>>;
    async fn compose_specialization<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        program: Program<'db>,
        source: R,
    ) -> RunResult<Specialization<'db>>;
}

/// Cloning copies retained handles without allocation or database operations.
pub(in crate::types) trait RetainedMappingSource<'run, 'db: 'run>:
    Sized + 'run
{
    type Effects<'call>: MappingSourceEffects<'run, 'db>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_>;

    /// Clones the source after admitting handle cloning, fixed transfers, and nonfinal retirement.
    /// The clone preserves the program and canonical routes. Providers and the driver retain the
    /// shared backing owners until mapping descendants drain, so retiring a clone only releases
    /// shared handles.
    async fn retained_clone(&self) -> RunResult<Self>;
}

pub(in crate::types) trait MappingSourceEffects<'run, 'db: 'run>:
    crate::types::generics::context_construction::ContextConstructionEffects<'db, Error = RunError>
{
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db>;

    async fn function_signature(&self, function: FunctionType<'db>) -> RunResult<&'db CallableSignature<'db>>;

    async fn function_has_separate_implementation(&self, literal: FunctionLiteral<'db>) -> RunResult<bool>;

    /// Builds one regular callable from the canonical last-definition signature when no stored implementation callables exist.
    async fn function_implementation_callable(&self, function: FunctionType<'db>) -> RunResult<CallableType<'db>>;

    async fn intern_mapped_function(&self, literal: FunctionLiteral<'db>, updated: Option<Box<UpdatedFunctionSignatures<'db>>>, descriptor_kind: Option<CallableTypeKind>) -> RunResult<FunctionType<'db>>;

    async fn owned_mapped_callable(&self, signatures: CallableSignature<'db>, kind: CallableTypeKind, deprecated: Option<OverloadLiteral<'db>>) -> RunResult<CallableType<'db>>;

    async fn finish_owned_parameters(&self, parameters: Vec<Parameter<'db>>, kind: ParametersKind<'db>) -> RunResult<Parameters<'db>>;

    async fn new_mapping_parameters(&self, capacity: usize) -> RunResult<Vec<Parameter<'db>>>;

    async fn push_mapping_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> RunResult<()>;

    async fn new_mapping_overloads(&self, capacity: usize) -> RunResult<SmallVec<[Signature<'db>; 1]>>;

    async fn push_mapping_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> RunResult<()>;

    async fn mapping_box_local<T, U>(&self, action: impl FnOnce() -> U) -> RunResult<U>;

    async fn mapping_parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> RunResult<ParameterKind<'db>>;

    async fn new_mapping_tuple_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>>;

    async fn push_mapping_tuple_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()>;

    async fn finish_mapping_tuple_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>>;

    async fn new_return_context_variables(&self, capacity: usize) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>>;

    async fn insert_return_context_variable(&self, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> RunResult<()>;

    async fn finish_return_context(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> RunResult<GenericContext<'db>>;

    /// Maps one retained context declaration with a new visitor and non-materializing stored
    /// specialization, honoring the supplied `specialize_self_domain` flag.
    async fn specialize_context_declaration(&self, variable: BoundTypeVarInstance<'db>, specialization: Specialization<'db>, specialize_self_domain: bool) -> RunResult<Type<'db>>;

    /// Resolves the occurrence's binding program before its bounds are read.
    /// This environment is for resolving bounds; mapping uses the source provider's current program.
    async fn retained_self_environment(&self, binding: crate::types::BindingContext<'db>) -> RunResult<ProgramEnvironment<'db>>;

    /// Maps a present upper bound or ordered constraints in the source provider's current mapping
    /// program, which must be the same program used by the caller's mapping visitor. This is distinct
    /// from the occurrence's binding environment used to resolve the bounds. Does not read
    /// materialization metadata from the specialization.
    async fn retained_self_bounds(&self, bounds: crate::types::TypeVarBoundOrConstraints<'db>, specialization: Specialization<'db>) -> RunResult<crate::types::TypeVarBoundOrConstraints<'db>>;

    /// Maps a declaration with an independent root, adding `delta` to occurrences selected by `generic_context`.
    async fn freshen_context_declaration(
        &self,
        variable: BoundTypeVarInstance<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
    ) -> RunResult<Type<'db>>;

    async fn finish_freshening_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> RunResult<GenericContext<'db>>;

    async fn freshening_bounds(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<crate::types::TypeVarBoundOrConstraints<'db>>>;

    async fn freshening_bound_default(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Type<'db>>>;

    async fn intern_freshening_constraints(&self, elements: &mut Vec<Type<'db>>) -> RunResult<crate::types::typevar::TypeVarConstraints<'db>>;

    async fn intern_freshening_variable(
        &self,
        identity: crate::types::typevar::TypeVarIdentity<'db>,
        bounds: Option<crate::types::typevar::TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<crate::types::TypeVarVariance>,
        default: Option<crate::types::typevar::TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>>;

    async fn intern_freshening_bound(
        &self,
        variable: TypeVarInstance<'db>,
        identity: crate::types::typevar::BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>>;

    async fn promotion_function_callable(&self, function: FunctionType<'db>) -> RunResult<Type<'db>>;

    async fn unavailable<T>(
        &self,
        mapping: OwnedTypeMapping<'run, 'db>,
        operation: MaterializationOperation,
    ) -> RunResult<T>;

    async fn promotion_scalar(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>>;

    async fn promotion_nominal_known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>>;

    async fn promotion_explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> RunResult<ClassType<'db>>;

    async fn promotion_intern_explicit_any(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<ExplicitAnyInstanceClass<'db>>;

    async fn promotion_numeric_union(
        &self,
        env: &ProgramEnvironment<'db>,
        union: KnownUnion,
    ) -> RunResult<Type<'db>>;

    async fn should_bind_self(
        &self,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool>;

    async fn bind_typevar(
        &self,
        variable: TypeVarInstance<'db>,
        binding: BindingContext<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>>;

    async fn typeform_argument(&self, typeform: TypeFormType<'db>) -> RunResult<Type<'db>>;

    async fn intern_typeform(&self, argument: Type<'db>) -> RunResult<Type<'db>>;

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>>;

    async fn construct_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>>;

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]>;

    async fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined>;

    async fn new_union(&self, env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>>;

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()>;

    async fn finish_union(
        &self,
        builder: UnionBuilder<'db>,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<Type<'db>>;

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>>;

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>>;

    async fn next_intersection(&self, elements: &mut Elements<'db>)
    -> RunResult<Option<Type<'db>>>;

    async fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<IntersectionBuilder<'db>>;

    async fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()>;

    async fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()>;

    async fn finish_intersection(&self, builder: IntersectionBuilder<'db>) -> RunResult<Type<'db>>;
    async fn specialization_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>>;
    async fn specialization_types(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]>;
    async fn specialization_variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>>;
    async fn specialization_materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>>;
    async fn specialization_tuple(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<TupleType<'db>>>;
    async fn intern_mapped_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>>;

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>>;

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>>;

    async fn intern_mapped_alias(
        &self,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericAlias<'db>>;
}

pub(in crate::types) async fn apply_mapping_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    ty: Type<'db>,
    _program: Program<'db>,
    mapping: OwnedTypeMapping<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<Type<'db>> {
    let source_effects = source.effects();
    let effects = crate::types::local_transfer::local_with_fixed_transfers_at(
        source_effects.endpoint(), 5, 0, || SourceMapping {
            endpoint: source_effects.endpoint(),
            visitor,
            source: &source,
            mapping,
            #[cfg(test)]
            input: (db, ty, mapping),
        },
    ).await?;
    let borrowed_mapping = effects
        .local(
            8,
            size_of::<OwnedTypeMapping<'run, 'db>>() * 2 + size_of::<TypeMapping<'_, 'db>>() * 2,
            || Ok(mapping.into_mapping()),
        )
        .await?;
    #[cfg(test)]
    observations::root(visitor, _program, mapping, TypeContext::default());
    #[cfg(test)]
    let _root_cache = observations::RootCacheLifetime::new(ty, mapping, visitor);
    #[cfg(test)]
    let _promotion_lifetime = match mapping {
        OwnedTypeMapping::PromoteRegular(mode) => Some(
            public_promotion_observations::MappingLifetime::new(db, visitor, mode),
        ),
        _ => None,
    };
    #[cfg(test)]
    let _freshening_lifetime = match mapping {
        OwnedTypeMapping::FreshenBoundTypeVars { generic_context, delta } => Some(
            crate::types::call::bind::constructor_matching::observations::MappingLifetime::new(db, visitor, generic_context, delta),
        ),
        _ => None,
    };
    ty.apply_type_mapping_with(
        db,
        &borrowed_mapping,
        TypeContext::default(),
        visitor,
        &effects,
    )
    .await
}

/// Maps a Self occurrence's present upper bound or ordered constraints with the supplied new mapping
/// visitor and non-materializing stored specialization with Self-domain recursion disabled.
/// The same visitor is retained across every constraint child and through interruption drainage.
pub(in crate::types) async fn apply_retained_bounds_with_retained<
    'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    bounds: crate::types::TypeVarBoundOrConstraints<'db>,
    program: Program<'db>,
    specialization: Specialization<'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<crate::types::TypeVarBoundOrConstraints<'db>> {
    let source_effects = source.effects();
    let effects = crate::types::local_transfer::local_with_fixed_transfers_at(
        source_effects.endpoint(), 8, size_of::<OwnedTypeMapping<'run, 'db>>(), || {
            let mapping = OwnedTypeMapping::Specialization { specialization, specialize_self_domain: false, materialization_kind: None };
            SourceMapping {
                endpoint: source_effects.endpoint(), visitor, source: &source, mapping,
                #[cfg(test)]
                input: (db, Type::unknown(), mapping),
            }
        },
    ).await?;
    #[cfg(test)]
    observations::root(visitor, program, effects.mapping, TypeContext::default());
    #[cfg(not(test))]
    let _ = program;
    effects.function_child(|| crate::types::typevar::retained_self::map_retained_bounds_with(db, bounds, &effects)).await
}

/// Maps a parameter list with one retained visitor shared by every annotation and eager default.
/// The caller allocates this root's visitor independently of the return type's visitor.
pub(in crate::types) async fn apply_parameter_mapping_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    parameters: &Parameters<'db>,
    _program: Program<'db>,
    mapping: OwnedTypeMapping<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<Parameters<'db>> {
    let source_effects = source.effects();
    let effects = crate::types::local_transfer::local_with_fixed_transfers_at(
        source_effects.endpoint(), 5, 0, || SourceMapping {
            endpoint: source_effects.endpoint(),
            visitor,
            source: &source,
            mapping,
            #[cfg(test)]
            input: (db, Type::unknown(), mapping),
        },
    ).await?;
    let borrowed_mapping = effects
        .function_local(Some(2), Some(0), || mapping.into_mapping())
        .await?;
    #[cfg(test)]
    observations::root(visitor, _program, mapping, TypeContext::default());
    effects.function_child(|| {
        map_parameters_with(db, parameters, &borrowed_mapping, TypeContext::default(), visitor, &effects)
    }).await
}

/// Maps signatures with one retained visitor, leaving callable interning to the caller.
/// The caller supplies a fresh retained visitor for each independent callable; descendants
/// reuse that visitor and its immutable mapping.
pub(in crate::types) async fn apply_callable_signature_mapping_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    signatures: &CallableSignature<'db>,
    _program: Program<'db>,
    mapping: OwnedTypeMapping<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<CallableSignature<'db>> {
    let source_effects = source.effects();
    let effects = crate::types::local_transfer::local_with_fixed_transfers_at(
        source_effects.endpoint(), 5, 0, || SourceMapping {
            endpoint: source_effects.endpoint(),
            visitor,
            source: &source,
            mapping,
            #[cfg(test)]
            input: (db, Type::unknown(), mapping),
        },
    ).await?;
    let borrowed_mapping = effects.function_local(Some(2), Some(0), || mapping.into_mapping()).await?;
    #[cfg(test)]
    observations::root(visitor, _program, mapping, TypeContext::default());
    effects.function_child(|| {
        crate::types::signatures::mapping::map_callable_signature_with(
            db, signatures, &borrowed_mapping, TypeContext::default(), visitor, &effects,
        )
    }).await
}

/// Maps one signature with a retained visitor shared by its annotation descendants.
/// Generic-context declarations allocate their own roots through the source provider.
pub(in crate::types) async fn apply_signature_mapping_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    _program: Program<'db>,
    mapping: OwnedTypeMapping<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<Signature<'db>> {
    let source_effects = source.effects();
    let effects = crate::types::local_transfer::local_with_fixed_transfers_at(
        source_effects.endpoint(), 5, 0, || SourceMapping {
            endpoint: source_effects.endpoint(),
            visitor,
            source: &source,
            mapping,
            #[cfg(test)]
            input: (db, Type::unknown(), mapping),
        },
    ).await?;
    let borrowed_mapping = effects.function_local(Some(2), Some(0), || mapping.into_mapping()).await?;
    #[cfg(test)]
    observations::root(visitor, _program, mapping, TypeContext::default());
    #[cfg(test)]
    let _freshening_lifetime = match mapping {
        OwnedTypeMapping::FreshenBoundTypeVars { generic_context, delta } => Some(
            crate::types::call::bind::constructor_matching::observations::MappingLifetime::new(db, visitor, generic_context, delta),
        ),
        _ => None,
    };
    effects.function_child(|| {
        crate::types::signatures::mapping::map_signature_with(
            db, signature, &borrowed_mapping, TypeContext::default(), visitor, &effects,
        )
    }).await
}

pub(in crate::types) async fn apply_class_base_mapping_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    base: ClassBase<'db>,
    program: Program<'db>,
    mapping: ClassBaseMapping<'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<ClassBase<'db>> {
    let source_effects = source.effects();
    let endpoint = source_effects.endpoint();
    let effects = endpoint
        .local_call(|| {
            endpoint.admit_work(2)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes:
                    size_of::<SourceClassBaseMapping<'_, 'owner, 'env, 'run, 'db, R>>() * 2,
            })?;
            endpoint.check_completion()?;
            Ok(SourceClassBaseMapping {
                endpoint,
                visitor,
                source: &source,
                program,
                mapping,
            })
        })
        .await;
    map_class_base_with(db, base, effects.mapping, visitor, &effects).await
}

struct SourceClassBaseMapping<'call, 'owner, 'env, 'run, 'db: 'run, R> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: &'call R,
    program: Program<'db>,
    mapping: ClassBaseMapping<'db>,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    ClassBaseMappingEffects<'db> for SourceClassBaseMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn checkpoint(&self, work: ClassBaseSpecializationWork) -> RunResult<()> {
        Ok(self
            .endpoint
            .local_call(|| {
                let quote = work.quote();
                self.endpoint.admit_work(quote.work)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quote.bytes,
                })?;
                self.endpoint.check_completion()
            })
            .await)
    }

    async fn map_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: ClassBaseMapping<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<GenericAlias<'db>> {
        // Address selection/comparison/branch (4), alias/type construction (2), mapping
        // call/dispatch/field initializers/construction (6), and pair/result construction (2).
        let (ty, mapping) = crate::types::local_transfer::local_with_fixed_transfers_at(
            self.endpoint,
            14,
            size_of::<ClassBaseMapping<'db>>() * 2
                + size_of::<GenericAlias<'db>>() * 2
                + size_of::<OwnedTypeMapping<'run, 'db>>() * 2
                + size_of::<Type<'db>>() * 2
                + size_of::<&ApplyTypeMappingVisitor<'_, 'db>>() * 4
                + size_of::<bool>() * 2,
            || {
                if !std::ptr::addr_eq(
                    std::ptr::from_ref(visitor),
                    std::ptr::from_ref(self.visitor),
                ) {
                    return Err(RunError::Contract("mapping visitor is not retained"));
                }
                Ok((Type::GenericAlias(alias), mapping.into_owned()))
            },
        )
        .await??;
        let source = self.source.retained_clone().await?;
        let mapped =
            apply_mapping_with_retained(db, ty, self.program, mapping, self.visitor, source)
                .await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes:
                        size_of::<Type<'db>>() * 2 + size_of::<GenericAlias<'db>>() * 2,
                })?;
                self.endpoint.check_completion()?;
                match mapped {
                    Type::GenericAlias(alias) => Ok(alias),
                    _ => Err(RunError::Contract(
                        "class-base mapping did not return a generic alias",
                    )),
                }
            })
            .await)
    }
}

struct SourceMapping<'call, 'owner, 'env, 'run, 'db: 'run, R> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: &'call R,
    mapping: OwnedTypeMapping<'run, 'db>,
    #[cfg(test)]
    input: (&'db dyn Db, Type<'db>, OwnedTypeMapping<'run, 'db>),
}

impl<'run, 'db: 'run, R: RetainedMappingSource<'run, 'db>> SourceMapping<'_, '_, '_, 'run, 'db, R> {
    async fn unavailable<T>(&self, operation: MaterializationOperation) -> RunResult<T> {
        self.source
            .effects()
            .unavailable(self.mapping, operation)
            .await
    }

    fn check_visitor(&self, supplied: &ApplyTypeMappingVisitor<'_, 'db>) -> RunResult<()> {
        if std::ptr::addr_eq(
            std::ptr::from_ref(supplied),
            std::ptr::from_ref(self.visitor),
        ) {
            Ok(())
        } else {
            Err(RunError::Contract(
                "mapping visitor is not retained",
            ))
        }
    }

    async fn local<T>(
        &self,
        work: usize,
        bytes: usize,
        operation: impl FnOnce() -> RunResult<T>,
    ) -> RunResult<T> {
        crate::types::local_transfer::local_with_fixed_transfers_at(
            self.endpoint, work, bytes, operation,
        ).await?
    }
}

struct MappingUnion<'db> {
    builder: UnionBuilder<'db>,
    #[cfg(test)]
    observation: observations::SetBuilderLifetime,
}

struct MappingIntersection<'db> {
    builder: IntersectionBuilder<'db>,
    #[cfg(test)]
    observation: observations::SetBuilderLifetime,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SetMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    type Union = MappingUnion<'db>;
    type Intersection = MappingIntersection<'db>;

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.source.effects().union_elements(union).await
    }

    async fn next_union(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, size_of::<Type<'db>>(), || {
            Ok(elements.next().copied())
        })
        .await
    }

    async fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined> {
        self.source.effects().union_recursively_defined(union).await
    }

    async fn new_union(&self, env: &ProgramEnvironment<'db>) -> RunResult<Self::Union> {
        let builder = self.source.effects().new_union(env).await?;
        Ok(MappingUnion {
            builder,
            #[cfg(test)]
            observation: observations::SetBuilderLifetime::new(observations::SetKind::Union),
        })
    }

    async fn new_structural_union(&self, _capacity: usize) -> RunResult<Self::Union> {
        self.unavailable(MaterializationOperation::Mode).await
    }

    async fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> RunResult<()> {
        self.source
            .effects()
            .union_add(&mut builder.builder, ty)
            .await?;
        #[cfg(test)]
        builder
            .observation
            .added(self.input.0, Some(builder.builder.elements_storage().0));
        Ok(())
    }

    async fn finish_union(
        &self,
        builder: Self::Union,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<Type<'db>> {
        let MappingUnion {
            builder,
            #[cfg(test)]
            mut observation,
        } = builder;
        let result = self
            .source
            .effects()
            .finish_union(builder, recursively_defined)
            .await?;
        #[cfg(test)]
        observation.completed();
        Ok(result)
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        self.source.effects().positive_elements(intersection).await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        self.source.effects().negative_elements(intersection).await
    }

    async fn next_intersection(
        &self,
        elements: &mut Elements<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.source.effects().next_intersection(elements).await
    }

    async fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Self::Intersection> {
        let builder = self.source.effects().new_intersection(env).await?;
        Ok(MappingIntersection {
            builder,
            #[cfg(test)]
            observation: observations::SetBuilderLifetime::new(observations::SetKind::Intersection),
        })
    }

    async fn new_structural_intersection(
        &self,
        _positive_capacity: usize,
    ) -> RunResult<Self::Intersection> {
        self.unavailable(MaterializationOperation::Mode).await
    }

    async fn add_positive(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        self.source
            .effects()
            .add_positive(&mut builder.builder, ty)
            .await?;
        #[cfg(test)]
        builder.observation.added(self.input.0, None);
        Ok(())
    }

    async fn add_negative(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        self.source
            .effects()
            .add_negative(&mut builder.builder, ty)
            .await?;
        #[cfg(test)]
        builder.observation.added(self.input.0, None);
        Ok(())
    }

    async fn finish_intersection(&self, builder: Self::Intersection) -> RunResult<Type<'db>> {
        let MappingIntersection {
            builder,
            #[cfg(test)]
            mut observation,
        } = builder;
        let result = self.source.effects().finish_intersection(builder).await?;
        #[cfg(test)]
        observation.completed();
        Ok(result)
    }

    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        SharedMappingEffects::map_type(self, db, ty, mapping, tcx, visitor).await
    }
}

#[cfg(test)]
type TupleObservation = observations::TupleBufferLifetime;
#[cfg(not(test))]
type TupleObservation = ();

struct TransformationControl<'call, 'run, 'db> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    #[cfg(test)]
    finish: Option<&'call observations::Finish<'run, 'db>>,
}

impl TypeTransformationControl for TransformationControl<'_, '_, '_> {
    type Error = RunError;

    fn checkpoint(&self, work: TypeTransformationWork) -> RunResult<()> {
        #[cfg(test)]
        if let Some(finish) = self.finish {
            finish.before_work(self.endpoint, work)?;
        }
        let quote = work
            .quote()
            .ok_or(RunError::Contract("transformation work quotation overflow"))?;
        // Active entries are popped when their scopes retire. Growth also owns a backing
        // allocation whose passive elements need no scan when that allocation is dropped.
        let disposal = match work {
            TypeTransformationWork::ActiveStorage { .. } | TypeTransformationWork::Grow { .. } => 1,
            _ => 0,
        };
        let units = quote
            .work_units
            .checked_add(disposal)
            .ok_or(RunError::Contract(
                "transformation disposal quotation overflow",
            ))?;
        self.endpoint.admit_work(units)?;
        if quote.requested_payload_bytes != 0 {
            #[cfg(test)]
            if let Some(finish) = self.finish {
                finish.before_resource(self.endpoint, work, quote.requested_payload_bytes)?;
            }
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: quote.requested_payload_bytes,
            })?;
        }
        self.endpoint.check_completion()
    }

    fn prepare_growth(&self, request: TypeTransformationGrowth) -> RunResult<Option<GrowthPlan>> {
        let plan = request.checked_plan().ok_or(RunError::Contract(
            "transformation growth quotation overflow",
        ))?;
        self.checkpoint(request.work(plan))?;
        Ok(Some(plan))
    }

    fn identity<'db>(&self, db: &'db dyn Db, ty: Type<'db>) -> RunResult<TypeIdentity<'db>> {
        self.endpoint.admit_work(1)?;
        self.endpoint.check_completion()?;
        match ty {
            Type::TypeForm(_) => Ok(ty.to_type_identity(db)),
            _ => Err(RunError::Contract(
                "mapping identity was not admitted",
            )),
        }
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SharedMappingStartEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Failure = RunError;

    async fn checkpoint(&self, work: MappingWork) -> RunResult<()> {
        let (units, bytes) = match work {
            MappingWork::ArgumentCapacity { width }
            | MappingWork::SpecializationPayload { width } => (
                width.checked_mul(3).and_then(|work| work.checked_add(4)),
                std::alloc::Layout::array::<Type<'db>>(width).ok().map(|layout| layout.size()),
            ),
            MappingWork::ArgumentPrefixCopy { len } => (len.checked_mul(2).and_then(|work| work.checked_add(1)), Some(0)),
            MappingWork::RootAdmission
            | MappingWork::TypeDispatch
            | MappingWork::TypeVarLookup
            | MappingWork::NominalPromotionClass
            | MappingWork::VarianceLookup
            | MappingWork::ScalarFallbackLookup
            | MappingWork::ArgumentAdvance
            | MappingWork::ChildRequest
            | MappingWork::ArgumentAppend
            | MappingWork::GenericAliasIntern
            | MappingWork::ExplicitAnyIntern
            | MappingWork::WrapperIntern
            | MappingWork::ResultPublication => (Some(1), Some(0)),
        };
        self.function_local(units, bytes, || ()).await
    }

    async fn admit_mode(&self, mapping: &TypeMapping<'_, 'db>) -> RunResult<()> {
        let supported = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<OwnedTypeMapping<'run, 'db>>() * 2,
                })?;
                self.endpoint.check_completion()?;
                Ok(self.mapping.recapture(mapping).is_some())
            })
            .await;
        if supported {
            Ok(())
        } else {
            self.unavailable(MaterializationOperation::Mode).await
        }
    }

    async fn expand_paramspecs(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.check_union_paramspec_expansion(db, ty, mapping, visitor).await
    }

    async fn nominal_known_class(
        &self,
        _db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.promotion_local(4, size_of::<R::Effects<'_>>(), || Ok(())).await?;
        self.source.effects().promotion_nominal_known_class(instance).await
    }

    async fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>> {
        self.local(2, size_of::<Type<'db>>() * 2, || {
            self.check_visitor(visitor)
        })
        .await?;
        match mapping {
            TypeMapping::FreshenBoundTypeVars { generic_context, delta } => {
                let mapped = self.freshen_typevar(db, variable, *generic_context, *delta).await?;
                self.function_local(Some(2), Some(0), || MappingStart::Complete(Type::TypeVar(mapped))).await
            }
            TypeMapping::BindSelf(binding) => self
                .function_local(Some(2), Some(0), || {
                    Ok(MappingStart::Continue(TypeVarMappingContinuation { binding: *binding, variable }))
                })
                .await?,
            TypeMapping::RescopeReturnCallables(_) => {
                self.function_local(Some(2), Some(0), || MappingStart::Complete(Type::TypeVar(variable))).await
            }
            TypeMapping::BindLegacyTypevars(_) => {
                self.local(1, 0, || {
                    Ok(MappingStart::Complete(variable.bind_legacy_typevars()))
                })
                .await
            }
            TypeMapping::ApplySpecialization(specialization) => Ok(MappingStart::Complete(
                self.specialize_typevar(db, variable, specialization)
                    .await?,
            )),
            _ => self.unavailable(MaterializationOperation::TypeVar).await,
        }
    }

    async fn begin_transformation<'v>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<TypeTransformerVisit<'db, MappingTransformationScope<'v, 'db>>> {
        let callable_operation = self.function_local(Some(4), Some(0), || match ty {
            Type::FunctionLiteral(_) => Some(MappingOperation::Function),
            Type::Callable(_) => Some(MappingOperation::Callable),
            _ => None,
        }).await?;
        if let Some(operation) = callable_operation {
            let supported = self.function_local(Some(12), Some(0), || {
                matches!(mapping, TypeMapping::Promote(_, PromotionKind::Regular) | TypeMapping::BindSelf(_) | TypeMapping::ApplySpecialization(ApplySpecialization::ReturnCallables(_) | ApplySpecialization::Specialization { .. } | ApplySpecialization::Single(..)) | TypeMapping::RescopeReturnCallables(_) | TypeMapping::FreshenBoundTypeVars { .. })
                    || matches!(
                        (ty, self.mapping),
                        (Type::Callable(_), OwnedTypeMapping::BindLegacyTypevars(_) | OwnedTypeMapping::Partial { .. })
                    )
            }).await?;
            if !supported {
                return self.unavailable(MaterializationOperation::Leaf(operation)).await;
            }
            // A cache hit needs no function-literal read. No RefCell borrow crosses the request,
            // and this visit has no active scope yet; enclosing transformation scopes stay active.
            let cached = self.function_local(
                Some(12),
                Some(size_of::<super::TypeMappingContinuation<'v, 'db>>() * 2),
                || {
                    self.check_visitor(visitor)?;
                    if visitor.transformer_cell(mapping).get().is_none() {
                        self.endpoint.admit_work(2)?;
                        self.endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: size_of::<TypeTransformer<'db, ApplyTypeMappingTag>>() * 3,
                        })?;
                    }
                    visitor.transformer(mapping).cached_result_with(
                        ty,
                        &TransformationControl {
                            endpoint: self.endpoint,
                            #[cfg(test)]
                            finish: None,
                        },
                    )
                },
            ).await??;
            if let Some(result) = cached {
                return Ok(TypeTransformerVisit::Ready(result));
            }
            let identity = match ty {
                Type::FunctionLiteral(function) => {
                    let request = self.function_local(Some(16), Some(0), || {
                        function.field_requests(self.endpoint.field_request_context()).literal()
                    }).await?;
                    let literal = self.function_child(|| async {
                        Ok(self.endpoint.read_field(request, &FixedMappingField).await)
                    }).await?;
                    self.function_local(Some(2), Some(0), || TypeIdentity::FunctionLiteral(literal)).await?
                }
                _ => self.function_local(Some(2), Some(0), || TypeIdentity::Other(ty)).await?,
            };
            return self.function_local(Some(8), Some(0), || {
                visitor.transformer(mapping).begin_uncached_visit_with(
                    ty,
                    identity,
                    &TransformationControl {
                        endpoint: self.endpoint,
                        #[cfg(test)]
                        finish: None,
                    },
                )
            }).await?;
        }
        let supported_identity = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                self.check_visitor(visitor)?;
                Ok(matches!(ty, Type::TypeForm(_)))
            })
            .await;
        if !supported_identity {
            return self.unavailable(MaterializationOperation::Identity).await;
        }
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                if visitor.transformer_cell(mapping).get().is_none() {
                    self.endpoint.admit_work(2)?;
                    self.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: size_of::<TypeTransformer<'db, ApplyTypeMappingTag>>() * 3,
                    })?;
                }
                self.endpoint.check_completion()?;
                visitor.transformer(mapping).begin_visit_with(
                    db,
                    ty,
                    &TransformationControl {
                        endpoint: self.endpoint,
                        #[cfg(test)]
                        finish: None,
                    },
                )
            })
            .await)
    }

    async fn legacy_leaf(
        &self,
        db: &'db dyn Db,
        _ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        match leaf {
            NativeMappingLeaf::Function(function) => {
                self.map_source_function(db, function, mapping, tcx, visitor).await
            }
            NativeMappingLeaf::Callable(callable) => {
                self.map_source_callable(db, callable, mapping, tcx, visitor).await
            }
            NativeMappingLeaf::KnownInstance(instance) => {
                map_known_instance_with(db, instance, mapping, tcx, visitor, self).await
            }
            NativeMappingLeaf::Float => {
                self.promotion_local(4, size_of::<R::Effects<'_>>(), || Ok(())).await?;
                self.source.effects().promotion_numeric_union(visitor.env, KnownUnion::Float).await
            }
            NativeMappingLeaf::Complex => {
                self.promotion_local(4, size_of::<R::Effects<'_>>(), || Ok(())).await?;
                self.source.effects().promotion_numeric_union(visitor.env, KnownUnion::Complex).await
            }
            leaf => self.unavailable(MaterializationOperation::Leaf(leaf.operation())).await,
        }
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    KnownInstanceMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn dispatch(&self) -> RunResult<()> {
        self.local(1, size_of::<KnownInstanceType<'db>>() * 2, || Ok(()))
            .await
    }

    async fn bind_typevar(
        &self,
        _db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
        binding_context: &BindingContext<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.source
            .effects()
            .bind_typevar(typevar, *binding_context)
            .await
    }

    async fn union_type(
        &self,
        _db: &'db dyn Db,
        _instance: UnionTypeInstance<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<UnionTypeInstance<'db>> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::KnownInstance,
        ))
        .await
    }

    async fn interned_inner(
        &self,
        _db: &'db dyn Db,
        _ty: InternedType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::KnownInstance,
        ))
        .await
    }

    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        SharedMappingEffects::map_type(self, db, ty, mapping, tcx, visitor).await
    }

    async fn intern_type(&self, _db: &'db dyn Db, _ty: Type<'db>) -> RunResult<InternedType<'db>> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::KnownInstance,
        ))
        .await
    }

    async fn callable(
        &self,
        _db: &'db dyn Db,
        _callable: CallableType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<CallableType<'db>> {
        self.unavailable(MaterializationOperation::Leaf(MappingOperation::Callable))
            .await
    }

    async fn method_wrapper(
        &self,
        _db: &'db dyn Db,
        _wrapper: MethodWrapper<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<MethodWrapper<'db>> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::KnownInstance,
        ))
        .await
    }

    async fn functools_partial(
        &self,
        _db: &'db dyn Db,
        _partial: FunctoolsPartialInstance<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<FunctoolsPartialInstance<'db>> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::KnownInstance,
        ))
        .await
    }

    async fn promote_range(
        &self,
        _db: &'db dyn Db,
        _instance: KnownInstanceType<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(MaterializationOperation::Leaf(MappingOperation::Promotion))
            .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SharedMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    async fn map_union(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.local(1, 0, || self.check_visitor(visitor)).await?;
        map_union_with(db, union, mapping, tcx, visitor, self, SetMappingFacts).await
    }

    async fn map_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.local(1, 0, || self.check_visitor(visitor)).await?;
        map_intersection_with(
            db,
            intersection,
            mapping,
            tcx,
            visitor,
            self,
            SetMappingFacts,
        )
        .await
    }

    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.local(1, 0, || self.check_visitor(visitor)).await?;
        let mapped = self.function_child(|| {
            map_tuple_with(db, tuple, mapping, tcx, visitor, self, TupleMappingFacts)
        }).await?;
        self.function_local(Some(2), Some(0), || Type::tuple(mapped)).await
    }

    async fn typeform_argument(
        &self,
        _db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> RunResult<Type<'db>> {
        self.source.effects().typeform_argument(typeform).await
    }

    async fn intern_typeform(&self, _db: &'db dyn Db, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.source.effects().intern_typeform(argument).await
    }

    async fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> RunResult<Type<'db>> {
        #[cfg(test)]
        let finish = observations::Finish::new(self.input, self.visitor, result);
        let mut scope = scope;
        // The future retains the active scope while a rejected callback drains its children.
        let prepared = self
            .endpoint
            .local_call(|| {
                #[cfg(test)]
                finish.before_finish(self.endpoint)?;
                self.endpoint.admit_work(1)?;
                let prepared = scope.prepare_finish_with(
                    result,
                    &TransformationControl {
                        endpoint: self.endpoint,
                        #[cfg(test)]
                        finish: Some(&finish),
                    },
                )?;
                #[cfg(test)]
                finish.after_prepare(self.endpoint)?;
                Ok(prepared)
            })
            .await;
        match prepared.try_commit() {
            Ok(result) => {
                #[cfg(test)]
                finish.committed();
                Ok(result)
            }
            Err(rejected) => {
                let result = self
                    .endpoint
                    .local_call(|| {
                        let _held = &rejected;
                        Err::<Type<'db>, _>(RunError::Contract(
                            "transformation finish preparation became stale",
                        ))
                    })
                    .await;
                drop(rejected);
                Ok(result)
            }
        }
    }

    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        let mapping = self
            .function_local(Some(2), Some(0), || {
                self.check_visitor(visitor)?;
                Ok::<_, RunError>(self.mapping.recapture(mapping))
            })
            .await??;
        let Some(mapping) = mapping else {
            return self.unavailable(MaterializationOperation::Mode).await;
        };
        let source = self.source.retained_clone().await?;
        let visitor = self.visitor;
        // Six captures are moved into the factory and future (12). Forming/retiring both
        // aggregates and invoking the factory cost 5; the borrowed effects adapter costs 6,
        // and its endpoint calls/reference transfers cost 4. Fixed transfers quote the actual
        // factory; demand separately admits its concrete task, future, and reply storage.
        let make = self
            .function_local(
                Some(27),
                Some(
                    size_of::<R::Effects<'_>>() * 2
                        + size_of::<&TaskEndpoint<'run, 'db>>() * 4,
                ),
                move || {
                    move || async move {
                        #[cfg(test)]
                        let _child = observations::child(db, visitor, mapping, tcx);
                        let source_effects = source.effects();
                        let endpoint = source_effects.endpoint();
                        let effects = crate::types::local_transfer::local_with_fixed_transfers_at(endpoint, 5, 0, || SourceMapping {
                            endpoint,
                            visitor,
                            source: &source,
                            mapping,
                            #[cfg(test)]
                            input: (db, ty, mapping),
                        }).await?;
                        let borrowed_mapping = effects.function_local(Some(2), Some(0), || mapping.into_mapping()).await?;
                        ty.apply_type_mapping_with(
                            db,
                            &borrowed_mapping,
                            tcx,
                            visitor,
                            &effects,
                        )
                        .await
                    }
                },
            )
            .await?;
        Ok(self.endpoint.child_call(|| async { self.endpoint.demand(make)?.await }).await)
    }

    async fn resume_legacy(
        &self,
        db: &'db dyn Db,
        continuation: LegacyTypeMappingContinuation<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        let (continuation, nominal_supported) = self
            .promotion_local(24, size_of::<OwnedTypeMapping<'run, 'db>>() * 2 + size_of::<bool>() * 4, || {
                self.check_visitor(visitor)?;
                let nominal_supported = match self.mapping {
                    OwnedTypeMapping::PromoteRegular(_)
                    | OwnedTypeMapping::BindSelf(_)
                    | OwnedTypeMapping::BindLegacyTypevars(_)
                    | OwnedTypeMapping::Partial { .. }
                    | OwnedTypeMapping::FreshenBoundTypeVars { .. }
                    | OwnedTypeMapping::Specialization { materialization_kind: None, .. }
                    | OwnedTypeMapping::Single { .. }
                    | OwnedTypeMapping::ReturnCallables(_)
                    | OwnedTypeMapping::RescopeReturnCallables(_) => true,
                    OwnedTypeMapping::Materialize(_)
                    | OwnedTypeMapping::Specialization { materialization_kind: Some(_), .. } => false,
                };
                Ok((continuation, nominal_supported))
            })
            .await?;
        match continuation {
            LegacyTypeMappingContinuation::GenericAlias(alias) => {
                let mapped = self.function_child(|| {
                    map_generic_alias_with(db, alias, mapping, tcx, visitor, self)
                }).await?;
                self.promotion_local(3, 0, || Ok(Type::GenericAlias(mapped))).await
            }
            LegacyTypeMappingContinuation::Promotion(ty) => self.promotion_leaf(ty).await,
            LegacyTypeMappingContinuation::Nominal(instance) if nominal_supported =>
            {
                self.map_source_nominal(db, instance, mapping, tcx, visitor).await
            }
            LegacyTypeMappingContinuation::TypeVar(continuation) => {
                let should_bind = self.source.effects().should_bind_self(
                    visitor.env, &continuation.binding, continuation.variable,
                ).await?;
                self.function_local(Some(2), Some(0), || continuation.finish(should_bind)).await
            }
            LegacyTypeMappingContinuation::Nominal(_)
            | LegacyTypeMappingContinuation::SubclassOf(_) => {
                self.unavailable(MaterializationOperation::LegacyContinuation).await
            }
        }
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    GenericAliasMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn structural(&self, mapping: &TypeMapping<'_, 'db>) -> RunResult<bool> {
        self.local(1, size_of::<TypeMapping<'_, 'db>>(), || {
            Ok(mapping.is_structural())
        })
        .await
    }

    async fn annotation(&self, context: TypeContext<'db>) -> RunResult<Option<Type<'db>>> {
        self.local(
            1,
            size_of::<TypeContext<'db>>() + size_of::<Option<Type<'db>>>() * 2,
            || Ok(context.annotation),
        )
        .await
    }

    async fn annotation_context(
        &self,
        _db: &'db dyn Db,
        _alias: GenericAlias<'db>,
        _annotation: Type<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::AnnotationContext,
        ))
        .await
    }

    async fn specialization(
        &self,
        _db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.source.effects().alias_specialization(alias).await
    }

    async fn map_specialization(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Specialization<'db>> {
        let mapping = self
            .local(2, size_of::<OwnedTypeMapping<'run, 'db>>() * 2, || {
                self.check_visitor(visitor)?;
                Ok(self.mapping.recapture(mapping))
            })
            .await?;
        let Some(mapping) = mapping else {
            return self.unavailable(MaterializationOperation::Mode).await;
        };
        specialization::map_specialization_with_retained(
            db,
            specialization,
            mapping,
            contexts,
            visitor,
            self.visitor,
            self.source,
            self.endpoint,
        )
        .await
    }

    async fn same_specialization(
        &self,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> RunResult<bool> {
        self.local(1, size_of::<Specialization<'db>>() * 2, || {
            Ok(left == right)
        })
        .await
    }

    async fn checkpoint(&self, work: MappingWork) -> RunResult<()> {
        SharedMappingStartEffects::checkpoint(self, work).await
    }

    async fn origin(
        &self,
        _db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> RunResult<StaticClassLiteral<'db>> {
        self.source.effects().alias_origin(alias).await
    }

    async fn intern(
        &self,
        _db: &'db dyn Db,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericAlias<'db>> {
        self.source
            .effects()
            .intern_mapped_alias(origin, specialization)
            .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    TupleBufferStorageEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    async fn new_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>> {
        self.function_child(|| async { self.source.effects().new_mapping_tuple_elements(capacity).await }).await
    }

    async fn push_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.function_child(|| async { self.source.effects().push_mapping_tuple_element(elements, ty).await }).await
    }

    async fn finish_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>> {
        self.function_child(|| async { self.source.effects().finish_mapping_tuple_elements(elements, variable).await }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    TupleMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    type Buffer = TupleBuffer<'db, TupleObservation>;
    type FixedContexts = ();

    async fn spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.source.effects().tuple_spec(tuple).await
    }

    async fn map_spec(
        &self,
        db: &'db dyn Db,
        spec: &TupleSpec<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<TupleSpec<'db>> {
        self.function_child(|| {
            map_tuple_spec_with(db, spec, mapping, tcx, visitor, self, TupleMappingFacts)
        }).await
    }

    async fn fixed_contexts(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        tcx: TypeContext<'db>,
        _len: usize,
    ) -> RunResult<()> {
        let absent = self
            .local(1, 0, || Ok(tcx.annotation.is_none()))
            .await?;
        if absent {
            Ok(())
        } else {
            self.unavailable(MaterializationOperation::Leaf(
                MappingOperation::AnnotationContext,
            ))
            .await
        }
    }

    async fn next_fixed(
        &self,
        _db: &'db dyn Db,
        elements: &mut slice::Iter<'_, Type<'db>>,
        _contexts: &mut (),
    ) -> RunResult<Option<(Type<'db>, TypeContext<'db>)>> {
        self.local(2, 0, || {
            Ok(elements
                .next()
                .copied()
                .map(|ty| (ty, TypeContext::default())))
        })
        .await
    }

    async fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(2, 0, || Ok(elements.next().copied())).await
    }

    async fn new_buffer(&self, capacity: usize) -> RunResult<Self::Buffer> {
        TupleBuffer::new(self.endpoint, self, capacity, || {
            #[cfg(test)]
            {
                observations::TupleBufferLifetime::new()
            }
        })
        .await
    }

    async fn push(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> RunResult<()> {
        buffer
            .push(self.endpoint, self, ty, |_observation, _len| {
                #[cfg(test)]
                _observation.pushed(self.input.0, _len);
            })
            .await
    }

    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        SharedMappingEffects::map_type(self, db, ty, mapping, tcx, visitor).await
    }

    async fn classify_variadic(
        &self,
        _db: &'db dyn Db,
        _original: BoundTypeVarInstance<'db>,
        _mapped: Type<'db>,
    ) -> RunResult<MappedTupleVariable<'db>> {
        self.unavailable(MaterializationOperation::TypeVar).await
    }

    async fn start_variable(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        mut buffer: Self::Buffer,
        variable: MappedTupleVariable<'db>,
    ) -> RunResult<Self::Buffer> {
        let MappedTupleVariable::Segment(variable) = variable else {
            return self.unavailable(MaterializationOperation::TypeVar).await;
        };
        buffer.start_variable(self.endpoint, variable).await?;
        Ok(buffer)
    }

    async fn finish_buffer(&self, mut buffer: Self::Buffer) -> RunResult<TupleSpec<'db>> {
        buffer
            .finish(self.endpoint, self, |_observation| {
                #[cfg(test)]
                _observation.completed();
            })
            .await
    }

    async fn intern_tuple(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        self.source.effects().construct_tuple(env, &spec).await
    }

    async fn intern_structural(
        &self,
        _db: &'db dyn Db,
        _original: TupleType<'db>,
        _spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        self.unavailable(MaterializationOperation::Mode).await
    }
}

#[cfg(test)]
pub(in crate::types) mod observations {
    use std::cell::Cell;

    use salsa::attempt_probe::{Incomplete, report_incomplete};
    use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
    use salsa::plumbing::AsId;

    use crate::types::constraints::control::GrowthPlan;
    use crate::types::cyclic::{
        TypeIdentity, TypeTransformationControl, TypeTransformationGrowth,
        TypeTransformationStorage, TypeTransformationWork, TypeTransformerVisit,
    };
    use crate::types::mapping::OwnedTypeMapping;
    use crate::types::{
        ApplyTypeMappingVisitor, BindingContext, MaterializationKind, PromotionMode, Type, TypeContext,
    };
    use crate::{Db, Program};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct Root {
        pub(in crate::types) environment: usize,
        pub(in crate::types) visitor: usize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct Snapshot {
        pub(in crate::types) root_count: usize,
        pub(in crate::types) roots: [Option<Root>; 8],
        pub(in crate::types) child_count: usize,
        pub(in crate::types) child_visitors: [Option<usize>; 64],
    }

    /// Distinguishes an unused transformer from an existing transformer's completed cache lookup.
    /// `Unavailable` means the observation failed or was unsupported; it does not establish absence.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum RootCacheState {
        NoTransformer,
        Absent,
        Present,
        Unavailable,
    }

    /// Records whether the mapped root left a completed result after its child scopes retire.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct RootCacheObservation {
        pub(in crate::types) callable: Option<salsa::Id>,
        pub(in crate::types) visitor: usize,
        pub(in crate::types) mapping: OwnedMappingSnapshot,
        pub(in crate::types) state: RootCacheState,
    }

    /// Counts all root observations while retaining only the first eight in `observations`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct RootCacheSnapshot {
        pub(in crate::types) count: usize,
        pub(in crate::types) observations: [Option<RootCacheObservation>; 8],
    }

    const EMPTY_ROOT_CACHE: RootCacheSnapshot = RootCacheSnapshot {
        count: 0,
        observations: [None; 8],
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum BindingContextSnapshot {
        Definition(salsa::Id),
        Synthetic(salsa::Id),
    }

    impl From<BindingContext<'_>> for BindingContextSnapshot {
        fn from(context: BindingContext<'_>) -> Self {
            match context {
                BindingContext::Definition(definition) => Self::Definition(definition.as_id()),
                BindingContext::Synthetic(program) => Self::Synthetic(program.as_id()),
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum OwnedMappingSnapshot {
        Partial {
            generic_context: salsa::Id,
            owner: usize,
            len: usize,
            skip: Option<usize>,
        },
        ReturnCallables { owner: usize },
        RescopeReturnCallables { owner: usize },
        Materialize(MaterializationKind),
        PromoteRegular(PromotionMode),
        BindLegacyTypevars(BindingContextSnapshot),
        FreshenBoundTypeVars { generic_context: salsa::Id, delta: u32 },
        BindSelf {
            replacement_plain_class: Option<salsa::Id>,
            replacement_class: Option<salsa::Id>,
            binding_context: Option<BindingContextSnapshot>,
        },
        Specialization {
            specialization: salsa::Id,
            specialize_self_domain: bool,
            materialization_kind: Option<MaterializationKind>,
        },
        /// Records the selected occurrence; replacement values are checked through mapping results.
        Single { variable: salsa::Id },
    }

    impl From<OwnedTypeMapping<'_, '_>> for OwnedMappingSnapshot {
        fn from(mapping: OwnedTypeMapping<'_, '_>) -> Self {
            match mapping {
                OwnedTypeMapping::Partial { generic_context, types, skip } => Self::Partial {
                    generic_context: generic_context.as_id(), owner: types.owner_identity(), len: types.len(), skip,
                },
                OwnedTypeMapping::ReturnCallables(values) => Self::ReturnCallables { owner: values.owner_identity() },
                OwnedTypeMapping::RescopeReturnCallables(values) => Self::RescopeReturnCallables { owner: values.owner_identity() },
                OwnedTypeMapping::Materialize(kind) => Self::Materialize(kind),
                OwnedTypeMapping::PromoteRegular(mode) => Self::PromoteRegular(mode),
                OwnedTypeMapping::BindLegacyTypevars(context) => Self::BindLegacyTypevars(context.into()),
                OwnedTypeMapping::FreshenBoundTypeVars { generic_context, delta } => Self::FreshenBoundTypeVars { generic_context: generic_context.as_id(), delta },
                OwnedTypeMapping::BindSelf(binding) => Self::BindSelf {
                    replacement_plain_class: match binding.ty {
                        Type::NominalInstance(instance) => match instance.visitor_kind() {
                            crate::types::instance::NominalVisitorKind::Class(
                                crate::types::instance::NominalInstanceClass::Plain(class),
                            ) => Some(class.as_id()),
                            crate::types::instance::NominalVisitorKind::None
                            | crate::types::instance::NominalVisitorKind::Tuple(_)
                            | crate::types::instance::NominalVisitorKind::Class(
                                crate::types::instance::NominalInstanceClass::InheritsFromExplicitAny(_),
                            ) => None,
                        },
                        _ => None,
                    },
                    replacement_class: binding.class_literal.map(|class| class.as_id()),
                    binding_context: binding.binding_context.map(Into::into),
                },
                OwnedTypeMapping::Specialization {
                    specialization,
                    specialize_self_domain,
                    materialization_kind,
                } => Self::Specialization {
                    specialization: specialization.as_id(),
                    specialize_self_domain,
                    materialization_kind,
                },
                OwnedTypeMapping::Single { variable, .. } => Self::Single { variable: variable.as_id() },
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct MappingInvocation {
        pub(in crate::types) visitor: usize,
        pub(in crate::types) program: Option<salsa::Id>,
        pub(in crate::types) mapping: OwnedMappingSnapshot,
        pub(in crate::types) default_context: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct MappingSnapshot {
        pub(in crate::types) root_count: usize,
        pub(in crate::types) roots: [Option<MappingInvocation>; 8],
        pub(in crate::types) child_count: usize,
        pub(in crate::types) children: [Option<MappingInvocation>; 64],
    }

    const EMPTY_MAPPINGS: MappingSnapshot = MappingSnapshot {
        root_count: 0,
        roots: [None; 8],
        child_count: 0,
        children: [None; 64],
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct TuplePush {
        pub(in crate::types) buffer: usize,
        pub(in crate::types) len: usize,
        pub(in crate::types) remaining: Option<usize>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct TupleSnapshot {
        pub(in crate::types) created: usize,
        pub(in crate::types) live: usize,
        pub(in crate::types) dropped: usize,
        pub(in crate::types) partial_drops: usize,
        pub(in crate::types) last_dropped_len: Option<usize>,
        pub(in crate::types) push_count: usize,
        pub(in crate::types) pushes: [Option<TuplePush>; 64],
    }

    const EMPTY_TUPLES: TupleSnapshot = TupleSnapshot {
        created: 0,
        live: 0,
        dropped: 0,
        partial_drops: 0,
        last_dropped_len: None,
        push_count: 0,
        pushes: [None; 64],
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum SetKind {
        Union,
        Intersection,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct SetMutation {
        pub(in crate::types) builder: usize,
        pub(in crate::types) kind: SetKind,
        pub(in crate::types) count: usize,
        pub(in crate::types) len: Option<usize>,
        pub(in crate::types) remaining: Option<usize>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct SetSnapshot {
        pub(in crate::types) created: usize,
        pub(in crate::types) live: usize,
        pub(in crate::types) dropped: usize,
        pub(in crate::types) partial_drops: usize,
        pub(in crate::types) last_dropped_len: Option<usize>,
        pub(in crate::types) mutation_count: usize,
        pub(in crate::types) mutations: [Option<SetMutation>; 64],
    }

    const EMPTY_SETS: SetSnapshot = SetSnapshot {
        created: 0,
        live: 0,
        dropped: 0,
        partial_drops: 0,
        last_dropped_len: None,
        mutation_count: 0,
        mutations: [None; 64],
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum SetCleanupEvent {
        ChildEntered {
            child: usize,
            live_builders: usize,
            remaining: Option<usize>,
        },
        ChildDropped {
            child: usize,
            live_builders: usize,
        },
        BuilderDropped {
            builder: usize,
        },
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct SetCleanupSnapshot {
        pub(in crate::types) event_count: usize,
        pub(in crate::types) events: [Option<SetCleanupEvent>; 64],
    }

    const EMPTY_SET_CLEANUP: SetCleanupSnapshot = SetCleanupSnapshot {
        event_count: 0,
        events: [None; 64],
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum CleanupBoundary {
        Finish,
        Cache,
        Growth,
        RehashKey,
        Resource,
        Acceptance,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum Lookup {
        Original,
        Mapped,
        Absent { active: usize },
        Unavailable,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct ScopeSnapshot {
        pub(in crate::types) root: Lookup,
        pub(in crate::types) active: Option<usize>,
        pub(in crate::types) cache_len: usize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) enum CleanupDrop {
        Child,
        Owner,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub(in crate::types) struct CleanupSnapshot {
        pub(in crate::types) finishes: usize,
        pub(in crate::types) queued: usize,
        pub(in crate::types) started: usize,
        pub(in crate::types) prepared: bool,
        pub(in crate::types) committed: bool,
        pub(in crate::types) work: Option<TypeTransformationWork>,
        pub(in crate::types) resource: Option<usize>,
        pub(in crate::types) child: Option<ScopeSnapshot>,
        pub(in crate::types) owner: Option<ScopeSnapshot>,
        pub(in crate::types) drop_count: usize,
        pub(in crate::types) drops: [Option<CleanupDrop>; 2],
    }

    thread_local! {
        static OBSERVATIONS: Cell<Snapshot> = const { Cell::new(Snapshot {
            root_count: 0,
            roots: [None; 8],
            child_count: 0,
            child_visitors: [None; 64],
        }) };
        static MAPPINGS: Cell<MappingSnapshot> = const { Cell::new(EMPTY_MAPPINGS) };
        static CANCEL_AT: Cell<Option<usize>> = const { Cell::new(None) };
        static TUPLES: Cell<TupleSnapshot> = const { Cell::new(EMPTY_TUPLES) };
        static SETS: Cell<SetSnapshot> = const { Cell::new(EMPTY_SETS) };
        static SET_CLEANUP: Cell<SetCleanupSnapshot> = const { Cell::new(EMPTY_SET_CLEANUP) };
        static CLEANUP_AT: Cell<Option<(CleanupBoundary, usize)>> = const { Cell::new(None) };
        static CLEANUP: Cell<CleanupSnapshot> = Cell::new(CleanupSnapshot::default());
        static ROOT_CACHE: Cell<RootCacheSnapshot> = const { Cell::new(EMPTY_ROOT_CACHE) };
    }

    pub(in crate::types) fn reset(cancel_at: Option<usize>) {
        OBSERVATIONS.set(Snapshot {
            root_count: 0,
            roots: [None; 8],
            child_count: 0,
            child_visitors: [None; 64],
        });
        MAPPINGS.set(EMPTY_MAPPINGS);
        CANCEL_AT.set(cancel_at);
        TUPLES.set(EMPTY_TUPLES);
        SETS.set(EMPTY_SETS);
        SET_CLEANUP.set(EMPTY_SET_CLEANUP);
        CLEANUP_AT.set(None);
        CLEANUP.set(CleanupSnapshot::default());
        ROOT_CACHE.set(EMPTY_ROOT_CACHE);
    }

    pub(in crate::types) fn snapshot() -> Snapshot {
        OBSERVATIONS.get()
    }

    /// Reads completed root-retirement observations without constructing or updating a cache.
    pub(in crate::types) fn root_cache_snapshot() -> RootCacheSnapshot {
        ROOT_CACHE.get()
    }

    pub(in crate::types) fn mapping_snapshot() -> MappingSnapshot {
        MAPPINGS.get()
    }

    pub(in crate::types) fn tuple_snapshot() -> TupleSnapshot {
        TUPLES.get()
    }

    pub(in crate::types) fn set_snapshot() -> SetSnapshot {
        SETS.get()
    }

    pub(in crate::types) fn set_cleanup_snapshot() -> SetCleanupSnapshot {
        SET_CLEANUP.get()
    }

    fn record_set_cleanup(event: SetCleanupEvent) {
        let mut snapshot = SET_CLEANUP.get();
        if let Some(slot) = snapshot.events.get_mut(snapshot.event_count) {
            *slot = Some(event);
        }
        snapshot.event_count += 1;
        SET_CLEANUP.set(snapshot);
    }

    pub(super) struct SetBuilderLifetime {
        id: usize,
        kind: SetKind,
        count: usize,
        len: Option<usize>,
        complete: bool,
    }

    impl SetBuilderLifetime {
        pub(super) fn new(kind: SetKind) -> Self {
            let mut snapshot = SETS.get();
            snapshot.created += 1;
            snapshot.live += 1;
            SETS.set(snapshot);
            Self {
                id: snapshot.created,
                kind,
                count: 0,
                len: None,
                complete: false,
            }
        }

        pub(super) fn added(&mut self, db: &dyn Db, len: Option<usize>) {
            self.count += 1;
            self.len = len;
            let mut snapshot = SETS.get();
            if let Some(slot) = snapshot.mutations.get_mut(snapshot.mutation_count) {
                *slot = Some(SetMutation {
                    builder: self.id,
                    kind: self.kind,
                    count: self.count,
                    len,
                    remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                });
            }
            snapshot.mutation_count += 1;
            SETS.set(snapshot);
        }

        pub(super) fn completed(&mut self) {
            self.complete = true;
        }
    }

    impl Drop for SetBuilderLifetime {
        fn drop(&mut self) {
            record_set_cleanup(SetCleanupEvent::BuilderDropped { builder: self.id });
            let mut snapshot = SETS.get();
            snapshot.live -= 1;
            snapshot.dropped += 1;
            snapshot.partial_drops += usize::from(!self.complete && self.count != 0);
            snapshot.last_dropped_len = self.len;
            SETS.set(snapshot);
        }
    }

    pub(super) struct TupleBufferLifetime {
        id: usize,
        len: usize,
        complete: bool,
    }

    impl TupleBufferLifetime {
        pub(super) fn new() -> Self {
            let mut snapshot = TUPLES.get();
            snapshot.created += 1;
            snapshot.live += 1;
            TUPLES.set(snapshot);
            Self {
                id: snapshot.created,
                len: 0,
                complete: false,
            }
        }

        pub(super) fn pushed(&mut self, db: &dyn Db, len: usize) {
            self.len = len;
            let mut snapshot = TUPLES.get();
            if let Some(slot) = snapshot.pushes.get_mut(snapshot.push_count) {
                *slot = Some(TuplePush {
                    buffer: self.id,
                    len,
                    remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                });
            }
            snapshot.push_count += 1;
            TUPLES.set(snapshot);
        }

        pub(super) fn completed(&mut self) {
            self.complete = true;
        }
    }

    impl Drop for TupleBufferLifetime {
        fn drop(&mut self) {
            let mut snapshot = TUPLES.get();
            snapshot.live -= 1;
            snapshot.dropped += 1;
            snapshot.partial_drops += usize::from(!self.complete);
            snapshot.last_dropped_len = Some(self.len);
            TUPLES.set(snapshot);
        }
    }

    pub(in crate::types) fn reset_cleanup(boundary: CleanupBoundary, finish: usize) {
        CLEANUP.set(CleanupSnapshot::default());
        CLEANUP_AT.set(Some((boundary, finish)));
    }

    pub(in crate::types) fn cleanup_snapshot() -> CleanupSnapshot {
        CLEANUP.get()
    }

    #[derive(Clone, Copy)]
    struct Witness<'run, 'db> {
        db: &'db dyn Db,
        visitor: &'run ApplyTypeMappingVisitor<'run, 'db>,
        input: Type<'db>,
        result: Type<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
    }

    #[derive(Default)]
    struct ReadOnlyProbe {
        cache_len: Cell<usize>,
    }

    /// Records the root's completed-cache state on drop without creating cache state.
    /// Retains a root's input and visitor until its mapping child and transformation scopes retire.
    pub(super) struct RootCacheLifetime<'owner, 'env, 'run, 'db> {
        input: Type<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    }

    impl<'owner, 'env, 'run, 'db> RootCacheLifetime<'owner, 'env, 'run, 'db> {
        pub(super) fn new(
            input: Type<'db>,
            mapping: OwnedTypeMapping<'run, 'db>,
            visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
        ) -> Self {
            Self { input, mapping, visitor }
        }
    }

    impl Drop for RootCacheLifetime<'_, '_, '_, '_> {
        fn drop(&mut self) {
            let mapping = self.mapping.into_mapping();
            let transformer = self.visitor.transformer_cell(&mapping).get();
            let state = match transformer {
                Some(transformer) => match transformer.cached_result_with(self.input, &ReadOnlyProbe::default()) {
                    Ok(Some(_)) => RootCacheState::Present,
                    Ok(None) => RootCacheState::Absent,
                    Err(_) => RootCacheState::Unavailable,
                },
                None => RootCacheState::NoTransformer,
            };
            let observation = RootCacheObservation {
                callable: self.input.as_callable().map(|callable| callable.as_id()),
                visitor: std::ptr::from_ref(self.visitor).addr(),
                mapping: self.mapping.into(),
                state,
            };
            let mut snapshot = ROOT_CACHE.get();
            if let Some(slot) = snapshot.observations.get_mut(snapshot.count) {
                *slot = Some(observation);
            }
            snapshot.count += 1;
            ROOT_CACHE.set(snapshot);
        }
    }

    impl TypeTransformationControl for ReadOnlyProbe {
        type Error = Option<usize>;

        fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Self::Error> {
            match work {
                TypeTransformationWork::CacheLookup { len } => self.cache_len.set(len),
                TypeTransformationWork::AncestorComparison
                | TypeTransformationWork::InlinePayload { .. } => {}
                TypeTransformationWork::ActiveStorage { len, .. } => return Err(Some(len)),
                _ => return Err(None),
            }
            Ok(())
        }

        fn identity<'db>(
            &self,
            _db: &'db dyn Db,
            ty: Type<'db>,
        ) -> Result<TypeIdentity<'db>, Self::Error> {
            match ty {
                Type::TypeForm(_) | Type::Never => Ok(TypeIdentity::Other(ty)),
                _ => Err(None),
            }
        }

        fn prepare_growth(
            &self,
            _request: TypeTransformationGrowth,
        ) -> Result<Option<GrowthPlan>, Self::Error> {
            Err(None)
        }
    }

    impl Witness<'_, '_> {
        fn observe(self) -> ScopeSnapshot {
            let mut snapshot = ScopeSnapshot {
                root: Lookup::Unavailable,
                active: None,
                cache_len: 0,
            };
            let mapping = self.mapping.into_mapping();
            let Some(transformer) = self.visitor.transformer_cell(&mapping).get() else {
                return snapshot;
            };
            let probe = ReadOnlyProbe::default();
            snapshot.root = match transformer.begin_visit_with(self.db, self.input, &probe) {
                Ok(TypeTransformerVisit::Ready(value)) if value == self.input => Lookup::Original,
                Ok(TypeTransformerVisit::Ready(value)) if value == self.result => Lookup::Mapped,
                Err(Some(active)) => Lookup::Absent { active },
                _ => Lookup::Unavailable,
            };
            // Only TypeForm types enter this transformer, so Never has no active or cached entry.
            // Refusing its active-storage admission observes depth without creating a scope.
            if let Err(active) = transformer.begin_visit_with(self.db, Type::Never, &probe) {
                snapshot.active = active;
            }
            snapshot.cache_len = probe.cache_len.get();
            snapshot
        }
    }

    pub(super) struct Finish<'run, 'db> {
        witness: Witness<'run, 'db>,
        boundary: Option<CleanupBoundary>,
    }

    impl<'run, 'db: 'run> Finish<'run, 'db> {
        pub(super) fn new(
            (db, input, mapping): (&'db dyn Db, Type<'db>, OwnedTypeMapping<'run, 'db>),
            visitor: &'run ApplyTypeMappingVisitor<'run, 'db>,
            result: Type<'db>,
        ) -> Self {
            let mut snapshot = CLEANUP.get();
            snapshot.finishes += 1;
            CLEANUP.set(snapshot);
            let boundary = CLEANUP_AT
                .get()
                .filter(|(_, finish)| *finish == snapshot.finishes)
                .map(|(boundary, _)| boundary);
            Self {
                witness: Witness {
                    db,
                    visitor,
                    input,
                    result,
                    mapping,
                },
                boundary,
            }
        }

        fn queue(&self, endpoint: &TaskEndpoint<'run, 'db>) -> RunResult<()> {
            if CLEANUP.get().queued != 0 {
                return Ok(());
            }
            let child = CleanupChild(self.witness);
            let _pending = endpoint.demand(move || async move {
                let _child = child;
                let mut snapshot = CLEANUP.get();
                snapshot.started += 1;
                CLEANUP.set(snapshot);
                Err::<(), _>(RunError::Contract("cleanup child was polled"))
            })?;
            let mut snapshot = CLEANUP.get();
            snapshot.queued += 1;
            CLEANUP.set(snapshot);
            if self.boundary != Some(CleanupBoundary::Acceptance) {
                report_incomplete(
                    self.witness.db,
                    if self.boundary == Some(CleanupBoundary::Resource) {
                        Incomplete::RequestedAllocation
                    } else {
                        Incomplete::Allowance
                    },
                );
            }
            Ok(())
        }

        pub(super) fn before_finish(&self, endpoint: &TaskEndpoint<'run, 'db>) -> RunResult<()> {
            if self.boundary == Some(CleanupBoundary::Finish) {
                self.queue(endpoint)?;
            }
            Ok(())
        }

        pub(super) fn before_work(
            &self,
            endpoint: &TaskEndpoint<'run, 'db>,
            work: TypeTransformationWork,
        ) -> RunResult<()> {
            let selected = matches!(
                (self.boundary, work),
                (
                    Some(CleanupBoundary::Cache),
                    TypeTransformationWork::CacheStorage { .. }
                ) | (
                    Some(CleanupBoundary::Growth),
                    TypeTransformationWork::Grow {
                        storage: TypeTransformationStorage::Cache,
                        ..
                    }
                ) | (
                    Some(CleanupBoundary::RehashKey),
                    TypeTransformationWork::RehashKey { .. }
                )
            );
            if selected {
                let mut snapshot = CLEANUP.get();
                snapshot.work = Some(work);
                CLEANUP.set(snapshot);
                self.queue(endpoint)?;
            }
            Ok(())
        }

        pub(super) fn before_resource(
            &self,
            endpoint: &TaskEndpoint<'run, 'db>,
            work: TypeTransformationWork,
            bytes: usize,
        ) -> RunResult<()> {
            if self.boundary == Some(CleanupBoundary::Resource)
                && matches!(
                    work,
                    TypeTransformationWork::Grow {
                        storage: TypeTransformationStorage::Cache,
                        ..
                    }
                )
            {
                let mut snapshot = CLEANUP.get();
                snapshot.work = Some(work);
                snapshot.resource = Some(bytes);
                CLEANUP.set(snapshot);
                self.queue(endpoint)?;
            }
            Ok(())
        }

        pub(super) fn after_prepare(&self, endpoint: &TaskEndpoint<'run, 'db>) -> RunResult<()> {
            if self.boundary.is_some() {
                let mut snapshot = CLEANUP.get();
                snapshot.prepared = true;
                CLEANUP.set(snapshot);
                if self.boundary == Some(CleanupBoundary::Acceptance) {
                    self.queue(endpoint)?;
                }
            }
            Ok(())
        }

        pub(super) fn committed(&self) {
            if self.boundary.is_some() {
                let mut snapshot = CLEANUP.get();
                snapshot.committed = true;
                CLEANUP.set(snapshot);
            }
        }
    }

    impl Drop for Finish<'_, '_> {
        fn drop(&mut self) {
            if self.boundary.is_some() {
                let mut snapshot = CLEANUP.get();
                snapshot.owner = Some(self.witness.observe());
                if let Some(slot) = snapshot.drops.get_mut(snapshot.drop_count) {
                    *slot = Some(CleanupDrop::Owner);
                }
                snapshot.drop_count += 1;
                CLEANUP.set(snapshot);
            }
        }
    }

    struct CleanupChild<'run, 'db>(Witness<'run, 'db>);

    impl Drop for CleanupChild<'_, '_> {
        fn drop(&mut self) {
            let mut snapshot = CLEANUP.get();
            snapshot.child = Some(self.0.observe());
            if let Some(slot) = snapshot.drops.get_mut(snapshot.drop_count) {
                *slot = Some(CleanupDrop::Child);
            }
            snapshot.drop_count += 1;
            CLEANUP.set(snapshot);
        }
    }

    pub(super) fn root(
        visitor: &ApplyTypeMappingVisitor<'_, '_>,
        program: Program<'_>,
        mapping: OwnedTypeMapping<'_, '_>,
        tcx: TypeContext<'_>,
    ) {
        let mut snapshot = OBSERVATIONS.get();
        if let Some(slot) = snapshot.roots.get_mut(snapshot.root_count) {
            *slot = Some(Root {
                environment: std::ptr::from_ref(visitor.env).addr(),
                visitor: std::ptr::from_ref(visitor).addr(),
            });
        }
        snapshot.root_count += 1;
        OBSERVATIONS.set(snapshot);
        let mut mappings = MAPPINGS.get();
        if let Some(slot) = mappings.roots.get_mut(mappings.root_count) {
            *slot = Some(MappingInvocation {
                visitor: std::ptr::from_ref(visitor).addr(),
                program: Some(program.as_id()),
                mapping: mapping.into(),
                default_context: tcx == TypeContext::default(),
            });
        }
        mappings.root_count += 1;
        MAPPINGS.set(mappings);
    }

    pub(super) struct MappingChildLifetime(usize);

    impl Drop for MappingChildLifetime {
        fn drop(&mut self) {
            record_set_cleanup(SetCleanupEvent::ChildDropped {
                child: self.0,
                live_builders: SETS.get().live,
            });
        }
    }

    pub(super) fn child<'db>(
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        mapping: OwnedTypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> MappingChildLifetime {
        if let OwnedTypeMapping::FreshenBoundTypeVars { generic_context, delta } = mapping {
            crate::types::call::bind::constructor_matching::observations::observe_mapping_child(db, visitor, generic_context, delta);
        }
        let mut snapshot = OBSERVATIONS.get();
        if let Some(slot) = snapshot.child_visitors.get_mut(snapshot.child_count) {
            *slot = Some(std::ptr::from_ref(visitor).addr());
        }
        snapshot.child_count += 1;
        OBSERVATIONS.set(snapshot);
        let mut mappings = MAPPINGS.get();
        if let Some(slot) = mappings.children.get_mut(mappings.child_count) {
            *slot = Some(MappingInvocation {
                visitor: std::ptr::from_ref(visitor).addr(),
                program: None,
                mapping: mapping.into(),
                default_context: tcx == TypeContext::default(),
            });
        }
        mappings.child_count += 1;
        MAPPINGS.set(mappings);
        record_set_cleanup(SetCleanupEvent::ChildEntered {
            child: snapshot.child_count,
            live_builders: SETS.get().live,
            remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
        });
        if CANCEL_AT.get() == Some(snapshot.child_count) {
            CANCEL_AT.set(None);
            db.cancellation_token().cancel();
        }
        MappingChildLifetime(snapshot.child_count)
    }
}
