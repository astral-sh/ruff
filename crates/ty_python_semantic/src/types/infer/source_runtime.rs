//! Canonical source-query access for an explicitly selected analysis root.

use std::rc::Rc;

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::SourceText;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{
    BorrowOrCopy, CallableRoute, CallableRouteProvider, ExecutionWork, FixedQueryKeys,
    InternedValues, NativeValueOperation, NativeValueQuote, QueryKeys, RegisteredRun,
    RegistryBuilder, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;
use ty_module_resolver::{KnownModule, Module, ModuleName};
use ty_python_core::definition::Definition;
use ty_python_core::expression::Expression;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::CallableAndCallExpr;
use ty_python_core::program::Program;
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{ExpressionNodeKey, PlaceTable, ProgramFile, SemanticIndex, UseDefMap};

use self::constructor_new::ConstructorNewKeyProfile;
use self::data_descriptor::DataDescriptorKeyProfile;
use super::builder::source_definition::controlled::{
    FixedFieldCopy, PreparedSource, SourceAccess, SourceEffects, SourceOperation,
};
use super::builder::source_definition::controlled::function_last_signature::last_signature_local;
use super::builder::source_definition::controlled::receiver_constraints::receiver_constraint_child_at;
use super::builder::source_expression::SourceExpressionOperation;
use super::builder::{ClassIdentity, OverloadIdentity};
use super::expression_context::register_expression_context_values;
use super::{local_quoted_with_fixed_transfers_at, local_with_fixed_transfers_at};
use crate::types::local_transfer::{boxed_future_with_fixed_transfers_at, generated_field_quote};
use crate::analysis::{AnalysisSession, ImplicitNameOperation, OperationId, PreparedAnalysisFile};
use crate::dunder_all::dunder_all_names_ingredient;
use crate::lint::RuleSelection;
use crate::place::implicit_globals::module_type_body_scope_ingredient;
use crate::place::{
    ConsideredDefinitions, PlaceAndQualifiers, RequiresExplicitReExport, place_by_id_ingredient,
};
use crate::prepared_host::PreparedHostFileReads;
use crate::reachability::{
    non_terminal_call_ingredient, non_terminal_call_initial, non_terminal_call_recover,
};
use crate::suppression::Suppressions;
use crate::types::abstract_methods::{
    AbstractMethod, abstract_methods_ingredient, might_be_explicitly_abstract_ingredient,
};
use crate::types::callable::{CallableType, CallableTypeKind, register_callable_values};
use crate::types::class::runtime::{GenericAliasValues, class_mro_literals_ingredient, register_generic_alias_values};
use crate::types::class::slots::{InstanceLayout, instance_layout_ingredient};
use crate::types::class::static_literal::{
    InheritanceCycle, class_decorators_ingredient, inheritance_cycle_inner_ingredient,
    try_metaclass_inner_ingredient,
};
use crate::types::class::{
    ClassInstanceFlags, CodeGeneratorKind, KnownClass, KnownClassArgument,
    KnownClassInstanceOperation, KnownClassLookupError, StaticClassLiteral,
    code_generator_of_static_class_ingredient, explicit_bases_ingredient,
    implicit_attribute_names_ingredient, inherited_class_context_ingredient,
    instance_flags_inner_ingredient, known_class_to_class_literal_ingredient,
    known_class_to_instance_ingredient, pep695_generic_context_ingredient, register_class_values, register_known_class_values,
    static_class_generic_context_ingredient, try_mro_unspecialized_ingredient,
};
use crate::types::class_member_lookup_ingredient;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::instance::ExplicitAnyInstanceClass;
use crate::types::instance::runtime::register_explicit_any_values;
use crate::types::descriptor::effects::descriptor_get_ingredient;
use crate::types::descriptor::{
    DescriptorRequest, DescriptorResult, register_descriptor_dispatch_values,
    register_descriptor_dispatches_values, register_descriptor_get_call_context_values,
};
use crate::types::enums::class_construction::register_enum_class_values;
use crate::types::enums::{
    EnumClassLiteral, EnumMetadata, enum_class_literal_ingredient, enum_metadata_ingredient,
};
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionLiteral, FunctionType,
    OverloadLiteral, UpdatedFunctionSignatures, function_last_definition_signature_ingredient,
    function_literal_signature_ingredient,
    overloads_and_implementation_ingredient, register_dataclass_transformer_values,
    register_function_values,
};
use crate::types::generics::Specialization;
use crate::types::generics::context_construction::{
    GenericContextValues, register_generic_context_values,
};
use crate::types::generics::defaults::{SpecializationValues, register_specialization_values};
use crate::types::literal::{StringLiteralType, intern_member_name_literal};
use crate::types::mapping::materialization::{
    MaterializationConfiguration, MaterializationKeyProfile,
};
use crate::types::mapping::source::MappingResourceAccess;
use crate::types::mapping::specialization::{
    SpecializationConfiguration, SpecializationEffects, SpecializationKeyProfile,
};
use crate::types::member_lookup::runtime_profile::{
    DescriptorLookupKeyProfile, PlaceConfiguration, PlaceLookupKeyProfile, quote_place_native_value,
};
use crate::types::member_lookup::{intern_member_lookup_key, register_member_lookup_values};
use crate::types::method::{BoundMethodReceiver, BoundMethodType, register_bound_method_values};
use crate::types::mro::source::source_alias_mro_ingredient;
use crate::types::mro::{Mro, StaticMroError};
use crate::types::narrow::{
    ExpressionNarrowingConstraints, expression_narrowing_constraints_ingredient,
};
use crate::types::overrides::runtime::{EffectiveVariableKindProfile, FunctionDefinitionProfile};
use crate::types::overrides::{
    VariableKind, effective_superclass_variable_kind_ingredient, is_function_definition_ingredient,
};
use crate::types::relation::source::resources::{RelationResourceAccess, SourceResources};
use crate::types::relation::source::retained::CheckerStorage;
use crate::types::relation::stable_storage::StableStorage;
use crate::types::set_theoretic::builder::{
    IntersectionPolarity, IntersectionSimplification, intersection_simplification_ingredient,
};
use crate::types::set_theoretic::{
    NegativeIntersectionElements, intersection_from_two_elements_ingredient,
    register_intersection_values, register_union_values, union_from_two_elements_ingredient,
};
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::signatures::{CallableSignature, Parameter, Signature};
use crate::types::tuple::{TupleSpec, TupleType, register_tuple_values};
use crate::types::typevar::construction::{
    TypeVarSetValues, TypeVarSetVariables, register_typevar_set_values,
};
use crate::types::typevar::runtime::{
    BoundTypeVarValues, TypeVarConstraintsValues, TypeVarIdentityValues, TypeVarInstanceValues,
    register_bound_typevar_values, register_typevar_identity_values,
    register_typevar_constraints_values, register_typevar_instance_values,
};
use crate::types::typevar::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraintsEvaluation,
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarSetInner,
    bound_typevar_default_ingredient, lazy_typevar_default_ingredient,
};
use crate::types::{
    ClassLiteral, ClassType, DescriptorArgumentComparison, DescriptorDispatch,
    DescriptorDispatches, DescriptorGetCallContext, DescriptorOrigin, GenericAlias, GenericContext,
    IntersectionType, LookupDunderNewConfiguration, LookupDunderNewQuery, LookupParts,
    MaterializationKind, MemberLookupError, MemberLookupKey,
    MemberLookupPolicy, MemberLookupResult, MemberMetadata, ModuleLiteralType,
    PropertyAccessorDefinitions, PropertyInstanceClass, PropertyInstanceType, RecursivelyDefined,
    ResolvedMember, Truthiness, Type, TypeFormType, TypePair, TypeVarVariance, UnionType,
    apply_specialization_ingredient, cached_materialization_ingredient, data_descriptor_ingredient,
    member_lookup_ingredient, register_type_pair_values, runtime_visibility_ingredient,
};
use crate::{AnalysisSettings, Db, FxIndexMap, FxOrderMap, FxOrderSet, ProgramEnvironment};

use super::{
    DefinitionInference, ExpressionInference, ExpressionWithContext, FunctionDecoratorInference,
    InferExpression, InferScope, ScopeInference, TypeContext,
    deferred_definition_inference_ingredient, definition_inference_ingredient,
    expression_inference_ingredient, function_decorator_inference_ingredient,
    scope_inference_ingredient,
};

#[cfg(test)]
pub(in crate::types::infer) mod tests;

mod abstract_methods;
mod class_member;
mod code_generator;
mod constructor_new;
mod data_descriptor;
mod descriptor;
mod implicit_names;
mod inheritance_cycle;
mod inner_metaclass;
mod instance_layout;
mod module_type_scope;
mod native_values;
mod override_variable_kind;
mod scope_maps;

trait DunderAllConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = ProgramFile<'a>,
        Output<'a> = Option<FxHashSet<Name>>,
    >
{
}

impl<C> DunderAllConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ProgramFile<'a>,
            Output<'a> = Option<FxHashSet<Name>>,
        >
{
}

// Keep the lifecycle count next to the complete field inventory so additions cannot leave it stale.
macro_rules! source_routes {
    (
        struct $name:ident<$run:lifetime, $db:lifetime: $bound:lifetime, $resources:ident> {
            $($field:ident: $ty:ty,)*
        }
    ) => {
        struct $name<$run, $db: $bound, $resources> {
            $($field: $ty,)*
        }

        impl<$run, $db: $bound, $resources> $name<$run, $db, $resources> {
            const FIELD_COUNT: usize = [$(stringify!($field)),*].len();
        }
    };
}

source_routes! {
struct SourceRoutes<'run, 'db: 'run, Resources> {
    resources: Resources,
    program: Program<'db>,
    scope: CallableRoute<'run, 'db, crate::types::infer::InferScopeTypesImplConfiguration>,
    expression:
        CallableRoute<'run, 'db, crate::types::infer::InferExpressionTypesImplConfiguration>,
    definition: CallableRoute<'run, 'db, crate::types::infer::InferDefinitionTypesConfiguration>,
    signature:
        CallableRoute<'run, 'db, crate::types::function::FunctionLiteralSignatureConfiguration>,
    last_definition_signature: CallableRoute<
        'run,
        'db,
        crate::types::function::FunctionLastDefinitionSignatureConfiguration,
    >,
    non_terminal_call:
        CallableRoute<'run, 'db, crate::reachability::AnalyzeNonTerminalCallConfiguration>,
    known_class:
        CallableRoute<'run, 'db, crate::types::class::KnownClassToClassLiteralConfiguration>,
    known_class_instance:
        CallableRoute<'run, 'db, crate::types::class::KnownClassToInstanceConfiguration>,
    class_generic_context: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::StaticClassGenericContextConfiguration,
    >,
    pep695_class_context: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::Pep695GenericContextInnerConfiguration,
    >,
    enum_metadata: CallableRoute<'run, 'db, crate::types::enums::EnumMetadataConfiguration>,
    enum_class_literal:
        CallableRoute<'run, 'db, crate::types::enums::EnumClassLiteralConfiguration>,
    enum_class_values: InternedValues<'db, EnumClassLiteral<'static>, ()>,
    static_mro: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::TryMroUnspecializedConfiguration,
    >,
    class_mro_literals: CallableRoute<'run, 'db, crate::types::ClassMroLiteralsConfiguration>,
    source_alias_mro:
        CallableRoute<'run, 'db, crate::types::mro::source::SourceAliasMroConfiguration>,
    deferred_definition:
        CallableRoute<'run, 'db, crate::types::infer::InferDeferredTypesConfiguration>,
    function_decorators:
        CallableRoute<'run, 'db, crate::types::infer::FunctionKnownDecoratorsConfiguration>,
    function_overloads: CallableRoute<
        'run,
        'db,
        crate::types::function::OverloadsAndImplementationInnerConfiguration,
    >,
    explicit_bases: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::ExplicitBasesInnerConfiguration,
    >,
    inheritance_cycle: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::InheritanceCycleInnerConfiguration,
    >,
    inner_metaclass: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::TryMetaclassInnerConfiguration,
    >,
    class_decorators:
        CallableRoute<'run, 'db, crate::types::class::static_literal::DecoratorsInnerConfiguration>,
    inherited_instance_flags: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::InstanceFlagsInnerConfiguration,
    >,
    bound_typevar_default:
        CallableRoute<'run, 'db, crate::types::typevar::BoundTypeVarDefaultTypeConfiguration>,
    lazy_typevar_default:
        CallableRoute<'run, 'db, crate::types::typevar::LazyDefaultUncheckedConfiguration>,
    inherited_class_context: CallableRoute<
        'run,
        'db,
        crate::types::class::static_literal::InheritedLegacyGenericContextInnerConfiguration,
    >,
    runtime_visibility: CallableRoute<'run, 'db, crate::types::MayExistAtRuntimeConfiguration>,
    member: CallableRoute<'run, 'db, crate::types::MemberLookupWithPolicyInnerConfiguration>,
    member_metadata_values: InternedValues<'db, MemberMetadata<'static>, ()>,
    member_error_values: InternedValues<'db, MemberLookupError<'static>, ()>,
    place_table: CallableRoute<'run, 'db, ty_python_core::PlaceTableConfiguration>,
    use_def_map: CallableRoute<'run, 'db, ty_python_core::UseDefMapConfiguration>,
    expression_narrowing: CallableRoute<
        'run,
        'db,
        crate::types::narrow::AllNarrowingConstraintsForExpressionConfiguration,
    >,
    pair_union:
        CallableRoute<'run, 'db, crate::types::set_theoretic::UnionFromTwoElementsConfiguration>,
    pair_intersection: CallableRoute<
        'run,
        'db,
        crate::types::set_theoretic::IntersectionFromTwoElementsConfiguration,
    >,
    pair_redundancy:
        CallableRoute<'run, 'db, crate::types::relation::IsRedundantWithImplConfiguration>,
    pair_owned_assignability: CallableRoute<
        'run,
        'db,
        crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration,
    >,
    intersection_simplification: CallableRoute<
        'run,
        'db,
        crate::types::set_theoretic::builder::SimplifyIntersectionPairImplConfiguration,
    >,
    intersection_simplification_keys: FixedQueryKeys<
        'db,
        crate::types::set_theoretic::builder::SimplifyIntersectionPairImplConfiguration,
    >,
    materialization: CallableRoute<'run, 'db, crate::types::CachedMaterializationConfiguration>,
    materialization_keys:
        QueryKeys<'db, crate::types::CachedMaterializationConfiguration, MaterializationKeyProfile>,
    specialization: CallableRoute<'run, 'db, crate::types::ApplySpecializationInnerConfiguration>,
    specialization_keys: QueryKeys<
        'db,
        crate::types::ApplySpecializationInnerConfiguration,
        SpecializationKeyProfile,
    >,
    place: CallableRoute<'run, 'db, crate::place::PlaceByIdConfiguration>,
    place_keys: QueryKeys<'db, crate::place::PlaceByIdConfiguration, PlaceLookupKeyProfile>,
    dunder_all: CallableRoute<'run, 'db, crate::dunder_all::DunderAllNamesConfiguration>,
    descriptor: CallableRoute<
        'run,
        'db,
        crate::types::descriptor::effects::TryCallDunderGetInnerConfiguration,
    >,
    class_member: CallableRoute<'run, 'db, crate::types::ClassMemberWithPolicyInnerConfiguration>,
    constructor_new: CallableRoute<'run, 'db, LookupDunderNewConfiguration>,
    constructor_new_keys: QueryKeys<'db, LookupDunderNewConfiguration, ConstructorNewKeyProfile>,
    code_generator:
        CallableRoute<'run, 'db, crate::types::class::CodeGeneratorOfStaticClassConfiguration>,
    instance_layout:
        CallableRoute<'run, 'db, crate::types::class::slots::InstanceLayoutConfiguration>,
    implicit_attribute_names: CallableRoute<
        'run,
        'db,
        crate::types::class::implicit_attributes::ImplicitAttributeNamesConfiguration,
    >,
    abstract_methods:
        CallableRoute<'run, 'db, crate::types::abstract_methods::AbstractMethodsConfiguration>,
    might_be_explicitly_abstract: CallableRoute<
        'run,
        'db,
        crate::types::abstract_methods::MightBeExplicitlyAbstractConfiguration,
    >,
    module_type_body_scope:
        CallableRoute<'run, 'db, crate::place::implicit_globals::ModuleTypeBodyScopeInnerConfiguration>,
    data_descriptor: CallableRoute<'run, 'db, crate::types::IsDataDescriptorImplConfiguration>,
    data_descriptor_keys:
        QueryKeys<'db, crate::types::IsDataDescriptorImplConfiguration, DataDescriptorKeyProfile>,
    effective_variable_kind: CallableRoute<
        'run,
        'db,
        crate::types::overrides::EffectiveSuperclassVariableKindConfiguration,
    >,
    effective_variable_kind_keys: QueryKeys<
        'db,
        crate::types::overrides::EffectiveSuperclassVariableKindConfiguration,
        EffectiveVariableKindProfile,
    >,
    function_definition:
        CallableRoute<'run, 'db, crate::types::overrides::IsFunctionDefinitionConfiguration>,
    function_definition_keys: QueryKeys<
        'db,
        crate::types::overrides::IsFunctionDefinitionConfiguration,
        FunctionDefinitionProfile,
    >,
    descriptor_keys: QueryKeys<
        'db,
        crate::types::descriptor::effects::TryCallDunderGetInnerConfiguration,
        DescriptorLookupKeyProfile,
    >,
    typeform_values: InternedValues<'db, TypeFormType<'static>, ()>,
    explicit_any_values: InternedValues<'db, ExplicitAnyInstanceClass<'static>, ()>,
    dataclass_transformer_values: InternedValues<'db, DataclassTransformerParams<'static>, ()>,
    typevar_identity_values: TypeVarIdentityValues<'db>,
    typevar_constraints_values: TypeVarConstraintsValues<'db>,
    typevar_instance_values:
        TypeVarInstanceValues<'db, crate::types::typevar::runtime::TypeVarInstanceMemoSchema<'db>>,
    bound_typevar_values:
        BoundTypeVarValues<'db, crate::types::typevar::runtime::BoundTypeVarMemoSchema<'db>>,
    generic_context_values: GenericContextValues<'db>,
    typevar_set_values: TypeVarSetValues<'db>,
    tuple_class: CallableRoute<'run, 'db, crate::types::tuple::ToClassTypeConfiguration>,
    specialization_values: SpecializationValues<'db>,
    generic_alias_values:
        GenericAliasValues<'db, crate::types::class::runtime::GenericAliasMemoSchema<'db>>,
}
}

impl<'run, 'db: 'run, Resources: Copy> SourceRoutes<'run, 'db, Resources> {
    fn allocate(
        registry: &RegistryBuilder<'run, 'db>,
        make: impl FnOnce() -> Self,
    ) -> RunResult<Rc<Self>> {
        // Each field is passive metadata or owns at most two Rc handles. Thirty-two logical
        // operations cover its transfer, retirement, and final-strong/weak bookkeeping. A
        // callable route can release its weak factory handle, but cannot destroy its provider.
        // The final eight operations cover the enclosing shared owner's bookkeeping. Payload
        // bytes are charged separately by allocate_shared_metadata.
        let lifecycle_work = Self::FIELD_COUNT
            .checked_mul(32)
            .and_then(|work| work.checked_add(8))
            .ok_or(RunError::Contract(
                "source route lifecycle quotation overflow",
            ))?;
        registry.allocate_shared_metadata(lifecycle_work, make)
    }
}

/// Quotes construction and transfer of the dataclass metadata input and its local result carriers.
/// The caller has already funded the box's backing; native interning funds its own request and commit.
fn dataclass_transformer_input_quote<F>(_: &F) -> RunResult<(usize, usize)> {
    type Fields<'db> = (DataclassTransformerFlags, Box<[Type<'db>]>);

    let bytes = size_of::<F>()
        .checked_add(size_of::<Option<F>>())
        .and_then(|bytes| bytes.checked_add(size_of::<Fields<'_>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Fields<'_>>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Fields<'_>>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<DataclassTransformerParams<'_>>>()))
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(RunError::Contract(
            "dataclass transformer input quotation overflow",
        ))?;
    // Retaining/consuming the factory (2), constructing/transferring fields (4), wrapping
    // the result (1), and retiring fixed wrappers (1) do not visit the box's elements.
    Ok((8, bytes))
}

struct SourceValues<'db> {
    function:
        InternedValues<'db, FunctionType<'static>, crate::types::function::FunctionMemoSchema<'db>>,
    overload: InternedValues<
        'db,
        OverloadLiteral<'static>,
        crate::types::function::OverloadMemoSchema<'db>,
    >,
    callable: InternedValues<'db, CallableType<'static>, ()>,
    bound_method: InternedValues<
        'db,
        BoundMethodType<'static>,
        crate::types::method::BoundMethodMemoSchema<'db>,
    >,
    descriptor_get_call_context: InternedValues<'db, DescriptorGetCallContext<'static>, ()>,
    descriptor_dispatch: InternedValues<'db, DescriptorDispatch<'static>, ()>,
    descriptor_dispatches: InternedValues<
        'db,
        DescriptorDispatches<'static>,
        crate::types::descriptor::DescriptorDispatchesMemoSchema<'db>,
    >,
    property: InternedValues<'db, PropertyInstanceType<'static>, ()>,
    module: InternedValues<'db, ModuleLiteralType<'static>, ()>,
    class: InternedValues<
        'db,
        StaticClassLiteral<'static>,
        crate::types::class::runtime::ClassMemoSchema<'db>,
    >,
    known_class: InternedValues<
        'db,
        KnownClassArgument<'static>,
        crate::types::class::KnownClassMemoSchema<'db>,
    >,
    member: InternedValues<
        'db,
        MemberLookupKey<'static>,
        crate::types::member_lookup::MemberLookupMemoSchema<'db>,
    >,
    tuple: InternedValues<'db, TupleType<'static>, crate::types::tuple::TupleMemoSchema<'db>>,
    string_literal: InternedValues<'db, StringLiteralType<'static>, ()>,
    union: InternedValues<'db, UnionType<'static>, ()>,
    intersection: InternedValues<'db, IntersectionType<'static>, ()>,
    type_pair: InternedValues<
        'db,
        TypePair<'static>,
        crate::types::TypePairMemoSchema<
            'db,
            crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration,
            crate::types::relation::WhenConstraintSetEquivalentToImplConfiguration,
            crate::types::relation::IsRedundantWithImplConfiguration,
            crate::types::constraints::IsPossiblyConstraintSetAssignableConfiguration,
            crate::types::set_theoretic::UnionFromTwoElementsConfiguration,
            crate::types::set_theoretic::IntersectionFromTwoElementsConfiguration,
        >,
    >,
    expression_context: InternedValues<
        'db,
        ExpressionWithContext<'static>,
        crate::types::infer::expression_context::ExpressionContextMemoSchema<'db>,
    >,
}

fn register_property_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, PropertyInstanceType<'static>, ()>> {
    registry.finite_interned_values_with_memos(PropertyInstanceType::ingredient(db.zalsa()), ())
}

fn register_module_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, ModuleLiteralType<'static>, ()>> {
    registry.finite_interned_values_with_memos(ModuleLiteralType::ingredient(db.zalsa()), ())
}

fn register_source_type_pair_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::TypePair<'static>,
        crate::types::TypePairMemoSchema<
            'db,
            crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration,
            crate::types::relation::WhenConstraintSetEquivalentToImplConfiguration,
            crate::types::relation::IsRedundantWithImplConfiguration,
            crate::types::constraints::IsPossiblyConstraintSetAssignableConfiguration,
            crate::types::set_theoretic::UnionFromTwoElementsConfiguration,
            crate::types::set_theoretic::IntersectionFromTwoElementsConfiguration,
        >,
    >,
> {
    register_type_pair_values(
        db,
        registry,
        crate::types::relation::owned_assignability_ingredient(db),
        crate::types::relation::owned_equivalence_ingredient(db),
        crate::types::relation::redundancy_ingredient(db),
        crate::types::constraints::possible_assignability_ingredient(db),
        union_from_two_elements_ingredient(db),
        intersection_from_two_elements_ingredient(db),
    )
}

/// Admits source-access construction and retirement before invoking the supplied factory.
/// Factories used here only assemble SourceQueryAccess's fields; additional allocation or owned
/// capture cleanup requires a separate quotation. The transfer helper keeps the action capturing
/// `make` alive through refused admission and child drainage, including on quotation overflow.
async fn create_source_access<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    endpoint: &TaskEndpoint<'run, 'db>,
    make: impl FnOnce(TaskEndpoint<'run, 'db>) -> A,
) -> RunResult<A> {
    // TaskEndpoint clones the queue, run context, and registry handles; the factory clones routes.
    // The provider and run retain those owners through child drainage, so these releases are nonfinal.
    // Four Rc clones/releases (8), admission/session/values copies (3), forming and retiring
    // Endpoint, TaskEndpoint, and SourceQueryAccess (6), three internal endpoint moves (3), and
    // invoking the factory (1) cost 21.
    // Aggregate operations exclude their fields; the transfer helper funds the returned access.
    //
    // size_of::<A>() bounds the initializer representations of those four handles plus admission,
    // session, and values. Four endpoint carriers bound the inner Endpoint's construction/return
    // and TaskEndpoint's clone-return/factory argument.
    // TaskEndpoint contains the private Endpoint, so its size bounds each inner carrier too.
    // The transfer helper separately quotes the action, completed access, and result carriers.
    let quote = size_of::<TaskEndpoint<'run, 'db>>()
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(size_of::<A>()))
        .map(|requested_bytes| (21, requested_bytes))
        .ok_or(RunError::Contract("source access byte quotation overflow"));
    local_quoted_with_fixed_transfers_at(endpoint, quote, || make(endpoint.clone())).await
}

struct SourceQueryAccess<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    endpoint: TaskEndpoint<'run, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<Resources: Copy> Clone for SourceQueryAccess<'_, '_, '_, Resources> {
    fn clone(&self) -> Self {
        Self {
            session: self.session,
            endpoint: self.endpoint.clone(),
            routes: self.routes.clone(),
            values: self.values,
        }
    }
}

impl<'run, 'session, 'db, Resources> SourceQueryAccess<'run, 'session, 'db, Resources> {
    async fn scope(
        &self,
        scope: ScopeId<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ScopeInference<'db>> {
        let key = self
            .endpoint
            .local_call(|| {
                if context.annotation.is_some() {
                    return self
                        .session
                        .unavailable(&self.endpoint, OperationId::ContextualScopeKey);
                }
                self.endpoint.admit_work(1)?;
                Ok(InferScope::Bare(scope).as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.scope, key)?.await })
            .await)
    }

    async fn expression(
        &self,
        expression: Expression<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ExpressionInference<'db>> {
        let contextual = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(context.annotation.is_some())
            })
            .await;
        let input = if contextual {
            #[cfg(test)]
            tests::contextual_expression::observe(
                tests::contextual_expression::Stage::BeforeIntern,
                self.session.db(),
            );
            let contextual = self
                .endpoint
                .intern_value(&self.values.expression_context, (expression, context))
                .await;
            #[cfg(test)]
            tests::contextual_expression::observe(
                tests::contextual_expression::Stage::AfterIntern,
                self.session.db(),
            );
            InferExpression::WithContext(contextual)
        } else {
            InferExpression::Bare(expression)
        };
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(input.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.expression, key)?.await })
            .await)
    }
}

pub(crate) fn run_file<'db>(
    session: &AnalysisSession<'_, 'db>,
    prepared: &PreparedAnalysisFile<'db>,
) -> RunResult<Result<Box<[Diagnostic]>, Diagnostic>> {
    let environments = StableStorage::new();
    let builders = StableStorage::new();
    let owners = StableStorage::new();
    let default_arguments = StableStorage::new();
    let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
    let mapping = StableStorage::new();
    let checkers = CheckerStorage::new();
    let resources = SourceResources::new(
        &environments,
        &builders,
        &owners,
        &mapping,
        &checkers,
        &default_arguments,
        &return_callables,
    );
    let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
    let (function, overload) = register_function_values(session.db(), &mut registry)?;
    let callable = register_callable_values(session.db(), &mut registry)?;
    let bound_method = register_bound_method_values(session.db(), &mut registry)?;
    let descriptor_get_call_context =
        register_descriptor_get_call_context_values(session.db(), &mut registry)?;
    let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
    let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
    let property = register_property_values(session.db(), &mut registry)?;
    let module = register_module_values(session.db(), &mut registry)?;
    let class = register_class_values(session.db(), &mut registry)?;
    let known_class = register_known_class_values(session.db(), &mut registry)?;
    let member = register_member_lookup_values(session.db(), &mut registry)?;
    let tuple = register_tuple_values(session.db(), &mut registry)?;
    let string_literal = registry.finite_interned_values_with_memos(
        StringLiteralType::ingredient(session.db().zalsa()),
        (),
    )?;
    let union = register_union_values(session.db(), &mut registry)?;
    let intersection = register_intersection_values(session.db(), &mut registry)?;
    let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
    let expression_context = register_expression_context_values(session.db(), &mut registry)?;
    let values = SourceValues {
        function,
        overload,
        callable,
        bound_method,
        descriptor_get_call_context,
        descriptor_dispatch,
        descriptor_dispatches,
        property,
        module,
        class,
        known_class,
        member,
        tuple,
        string_literal,
        union,
        intersection,
        type_pair,
        expression_context,
    };
    let (run, routes) = register(session, prepared, registry, &values, resources)?;
    let values = &values;
    run.run(|endpoint| async move {
        let access = SourceQueryAccess {
            session,
            endpoint,
            routes,
            values,
        };
        SourceEffects::new(&access, session.program())
            .check_file(prepared.program_file())
            .await
    })
}

pub(crate) fn run_expression<'db>(
    session: &AnalysisSession<'_, 'db>,
    prepared: &PreparedAnalysisFile<'db>,
    expression: Expression<'db>,
    expression_key: ExpressionNodeKey,
) -> RunResult<Type<'db>> {
    let environments = StableStorage::new();
    let builders = StableStorage::new();
    let owners = StableStorage::new();
    let default_arguments = StableStorage::new();
    let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
    let mapping = StableStorage::new();
    let checkers = CheckerStorage::new();
    let resources = SourceResources::new(
        &environments,
        &builders,
        &owners,
        &mapping,
        &checkers,
        &default_arguments,
        &return_callables,
    );
    let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
    let (function, overload) = register_function_values(session.db(), &mut registry)?;
    let callable = register_callable_values(session.db(), &mut registry)?;
    let bound_method = register_bound_method_values(session.db(), &mut registry)?;
    let descriptor_get_call_context =
        register_descriptor_get_call_context_values(session.db(), &mut registry)?;
    let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
    let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
    let property = register_property_values(session.db(), &mut registry)?;
    let module = register_module_values(session.db(), &mut registry)?;
    let class = register_class_values(session.db(), &mut registry)?;
    let known_class = register_known_class_values(session.db(), &mut registry)?;
    let member = register_member_lookup_values(session.db(), &mut registry)?;
    let tuple = register_tuple_values(session.db(), &mut registry)?;
    let string_literal = registry.finite_interned_values_with_memos(
        StringLiteralType::ingredient(session.db().zalsa()),
        (),
    )?;
    let union = register_union_values(session.db(), &mut registry)?;
    let intersection = register_intersection_values(session.db(), &mut registry)?;
    let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
    let expression_context = register_expression_context_values(session.db(), &mut registry)?;
    let values = SourceValues {
        function,
        overload,
        callable,
        bound_method,
        descriptor_get_call_context,
        descriptor_dispatch,
        descriptor_dispatches,
        property,
        module,
        class,
        known_class,
        member,
        tuple,
        string_literal,
        union,
        intersection,
        type_pair,
        expression_context,
    };
    let (run, routes) = register(session, prepared, registry, &values, resources)?;
    let values = &values;
    run.run(|endpoint| async move {
        let access = SourceQueryAccess {
            session,
            endpoint,
            routes,
            values,
        };
        let inference = access
            .expression(expression, TypeContext::default())
            .await?;
        Ok(access
            .endpoint
            .local_call(|| {
                // A frozen map has finite size; this quote covers its scalar lookup and fallback.
                access
                    .endpoint
                    .admit_work(inference.expressions.iter().len().saturating_add(1))?;
                access.endpoint.check_completion()?;
                Ok(inference.expression_type(expression_key))
            })
            .await)
    })
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> SourceAccess<'run, 'db> for SourceQueryAccess<'run, 'session, 'db, Resources>
{
    type Resources = Resources;

    fn resources(&self) -> Self::Resources {
        self.routes.resources
    }

    fn db(&self) -> &'db dyn Db {
        self.session.db()
    }
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        #[cfg(test)]
        tests::shared_routes::observe_access(Rc::as_ptr(&self.routes).addr());
        &self.endpoint
    }

    fn retained_clone_quote() -> RunResult<(usize, usize)> {
        // Four Rc clones/releases (8), admission/session/values copies (3), forming and retiring
        // Endpoint, TaskEndpoint, and SourceQueryAccess (6), two internal endpoint moves (2), and
        // the access/TaskEndpoint/Endpoint clone calls (3) cost 22. Providers retain routes;
        // RegisteredRun retains the registry and the driver retains queue/context through drain.
        // Those retained owners make all four releases nonfinal.
        //
        // Self bounds the seven leaf initializer representations. Four TaskEndpoint carriers
        // bound the inner Endpoint's construction/return and TaskEndpoint's construction/return.
        // The caller quotes completed access carriers; these bytes cover only clone internals.
        size_of::<TaskEndpoint<'run, 'db>>()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(size_of::<Self>()))
            .map(|bytes| (22, bytes))
            .ok_or(RunError::Contract("retained source access quotation overflow"))
    }

    async fn should_check_file(&self, file: File) -> RunResult<bool> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint.admit_work(24)?;
                source
                    .host_reads()
                    .should_check_file(self.endpoint.clone())?
                    .await
            })
            .await)
    }

    async fn rule_selection(&self, file: File) -> RunResult<&'db RuleSelection> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint.admit_work(24)?;
                source
                    .host_reads()
                    .rule_selection(self.endpoint.clone())?
                    .await
            })
            .await)
    }

    async fn verbose(&self, file: File) -> RunResult<bool> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(self
            .endpoint
            .child_call(|| async {
                let bytes = size_of::<(
                    [TaskEndpoint<'run, 'db>; 4],
                    [(Rc<dyn PreparedHostFileReads<'db> + 'db>, TaskEndpoint<'run, 'db>); 3],
                )>();
                let (host, child) = local_with_fixed_transfers_at(
                    &self.endpoint,
                    45,
                    bytes,
                    || (source.host_reads(), self.endpoint.clone()),
                )
                .await?;
                host.verbose(child)?.await
            })
            .await)
    }

    async fn analysis_settings(&self, file: File) -> RunResult<&'db AnalysisSettings> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint.admit_work(24)?;
                source
                    .host_reads()
                    .analysis_settings(self.endpoint.clone())?
                    .await
            })
            .await)
    }

    async fn source_text(&self, file: File) -> RunResult<SourceText> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(source.read_source_text(&self.endpoint).await)
    }

    async fn suppressions(&self, file: File) -> RunResult<&'db Suppressions> {
        let source = self
            .session
            .source(&self.endpoint, file, self.routes.program)
            .await?;
        Ok(source.read_suppressions(&self.endpoint).await)
    }

    async fn place_table(&self, scope: ScopeId<'db>) -> RunResult<&'db PlaceTable> {
        let table = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.place_table, scope.as_id())?
                    .await
            })
            .await;
        Ok(table.as_ref())
    }

    async fn use_def_map(&self, scope: ScopeId<'db>) -> RunResult<&'db UseDefMap<'db>> {
        let table = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.use_def_map, scope.as_id())?
                    .await
            })
            .await;
        Ok(table.as_ref())
    }

    async fn expression_narrowing_constraints(
        &self,
        expression: Expression<'db>,
    ) -> RunResult<&'db ExpressionNarrowingConstraints<'db>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.expression_narrowing, expression.as_id())?
                    .await
            })
            .await)
    }

    async fn semantic_index(&self, file: ProgramFile<'db>) -> RunResult<&'db SemanticIndex<'db>> {
        self.session.read_semantic_index(&self.endpoint, file).await
    }

    async fn parsed_module(&self, file: ProgramFile<'db>) -> RunResult<ParsedModuleRef> {
        let fields = file.read_fields(self.endpoint.field_request_context());
        let python_file = self
            .endpoint
            .read_field(fields.python_file(), &BorrowOrCopy)
            .await;
        let file = self
            .endpoint
            .read_field(
                python_file
                    .read_fields(self.endpoint.field_request_context())
                    .file(),
                &BorrowOrCopy,
            )
            .await;
        let program = self
            .endpoint
            .read_field(fields.program(), &BorrowOrCopy)
            .await;
        self.endpoint
            .local_call(|| self.endpoint.admit_work(8))
            .await;
        let source = self.session.source(&self.endpoint, file, program).await?;
        let module = source.read_parsed_module(&self.endpoint).await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(8)?;
                self.endpoint.check_completion()?;
                if module != source.parsed_module().module() {
                    return Err(RunError::Contract("prepared parser identity changed"));
                }
                Ok(source.parsed_module().clone())
            })
            .await)
    }

    async fn scope(
        &self,
        scope: ScopeId<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ScopeInference<'db>> {
        SourceQueryAccess::scope(self, scope, context).await
    }

    async fn expression(
        &self,
        expression: Expression<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ExpressionInference<'db>> {
        SourceQueryAccess::expression(self, expression, context).await
    }

    async fn definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                let demand = self
                    .endpoint
                    .fetch_ref(&self.routes.definition, definition.as_id())?;
                #[cfg(test)]
                let demand =
                    tests::quoted_annotations::observe_definition_request(definition.as_id(), demand);
                demand.await
            })
            .await)
    }

    async fn deferred_definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        let file = SourceEffects::new(self, self.routes.program)
            .definition_file(definition)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("deferred definition program is foreign"));
                }
                Ok(definition.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.deferred_definition, key)?
                    .await
            })
            .await)
    }

    async fn function_known_decorators(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db FunctionDecoratorInference<'db>> {
        let file = SourceEffects::new(self, self.routes.program)
            .definition_file(definition)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("function decorators program is foreign"));
                }
                Ok(definition.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.function_decorators, key)?
                    .await
            })
            .await)
    }

    async fn function_overloads(
        &self,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<&'db (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)> {
        let scope = self
            .endpoint
            .read_field(
                last_definition
                    .field_requests(self.endpoint.field_request_context())
                    .body_scope(),
                &BorrowOrCopy,
            )
            .await;
        let file = SourceEffects::new(self, self.routes.program)
            .scope_file(scope)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("function overloads program is foreign"));
                }
                Ok(last_definition.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.function_overloads, key)?
                    .await
            })
            .await)
    }

    async fn runtime_visibility(&self, definition: Definition<'db>) -> RunResult<bool> {
        let file = SourceEffects::new(self, self.routes.program)
            .definition_file(definition)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("runtime visibility program is foreign"));
                }
                Ok(definition.as_id())
            })
            .await;
        let visible = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.runtime_visibility, key)?
                    .await
            })
            .await;
        Ok(*visible)
    }

    async fn member_lookup_key(
        &self,
        ty: Type<'db>,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupKey<'db>> {
        Ok(intern_member_lookup_key(
            &self.endpoint,
            &self.values.member,
            self.routes.program,
            ty,
            name,
            policy,
        )
        .await)
    }

    async fn member_lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        let name = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(name.clone())
            })
            .await;
        let key = intern_member_lookup_key(
            &self.endpoint,
            &self.values.member,
            self.routes.program,
            ty,
            name,
            policy,
        )
        .await;
        let member = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.member, key.as_id())?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*member)
            })
            .await)
    }

    async fn class_member_lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let name = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(name.clone())
            })
            .await;
        let key = intern_member_lookup_key(
            &self.endpoint,
            &self.values.member,
            self.routes.program,
            ty,
            name,
            policy,
        )
        .await;
        let member = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.class_member, key.as_id())?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*member)
            })
            .await)
    }

    async fn constructor_new_member(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("constructor new program is foreign"));
                }
                Ok(())
            })
            .await;
        let effects = SourceEffects::new(self, program);
        let fields = effects.initialize_value(|| (program, ty)).await?;
        let key = self
            .endpoint
            .intern_query_key(&self.routes.constructor_new_keys, fields)
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.constructor_new, key)?
                    .await
            })
            .await;
        effects.initialize_value(|| *value).await
    }

    async fn descriptor_get(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        let key = self
            .endpoint
            .intern_query_key(
                &self.routes.descriptor_keys,
                (
                    self.routes.program,
                    request.ty,
                    request.instance,
                    request.owner,
                ),
            )
            .await;
        let result = self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.descriptor, key)?.await })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint
                    .admit_work(size_of::<DescriptorResult<'db>>() * 2 + 1)?;
                self.endpoint.check_completion()?;
                Ok(*result)
            })
            .await)
    }

    async fn member_result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        let needs_metadata = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                Ok(parts.properties.is_some() || parts.descriptor != DescriptorOrigin::default())
            })
            .await;
        let member = if needs_metadata {
            ResolvedMember::WithMetadata(
                self.endpoint
                    .intern_value(
                        &self.routes.member_metadata_values,
                        (parts.member, parts.properties, parts.descriptor),
                    )
                    .await,
            )
        } else {
            ResolvedMember::Plain(parts.member)
        };
        Ok(match parts.error {
            Some(error) => Err(self
                .endpoint
                .intern_value(&self.routes.member_error_values, (member, error))
                .await),
            None => Ok(member),
        })
    }

    async fn place_by_id(
        &self,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let file = SourceEffects::new(self, self.routes.program)
            .scope_file(scope)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("place lookup program is foreign"));
                }
                Ok(())
            })
            .await;
        let key = self
            .endpoint
            .intern_query_key(
                &self.routes.place_keys,
                (scope, place, reexport, considered),
            )
            .await;
        let place = self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.place, key)?.await })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*place)
            })
            .await)
    }

    async fn module_type_body_scope(
        &self,
        program: Program<'db>,
    ) -> RunResult<Option<ScopeId<'db>>> {
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("module type scope program is foreign"));
                }
                Ok(program.as_id())
            })
            .await;
        let scope = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.module_type_body_scope, key)?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*scope)
            })
            .await)
    }

    async fn dunder_all_names(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<&'db Option<FxHashSet<Name>>> {
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("dunder all program is foreign"));
                }
                Ok(file.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.dunder_all, key)?.await })
            .await)
    }

    async fn overload_literal(
        &self,
        identity: OverloadIdentity<'_, 'db>,
    ) -> RunResult<OverloadLiteral<'db>> {
        let fields = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok((
                    identity.name.clone(),
                    identity.known,
                    identity.body_scope,
                    identity.decorators,
                    None,
                    identity.dataclass_transformer,
                    identity.has_return_annotation,
                ))
            })
            .await;
        Ok(self
            .endpoint
            .intern_value(&self.values.overload, fields)
            .await)
    }

    async fn function_type(&self, literal: FunctionLiteral<'db>) -> RunResult<FunctionType<'db>> {
        Ok(self
            .endpoint
            .intern_value(&self.values.function, (literal, None, None))
            .await)
    }

    async fn intern_function_type(
        &self,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 4, 0, || {
            (literal, updated, descriptor_kind)
        })
        .await?;
        Ok(self
            .endpoint
            .intern_value(&self.values.function, fields)
            .await)
    }

    async fn intern_typevar_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> RunResult<TypeVarIdentity<'db>> {
        let name = self
            .endpoint
            .local_call(|| {
                let work = name
                    .len()
                    .checked_add(1)
                    .ok_or(RunError::Contract("type variable name copy quote overflow"))?;
                let requested_bytes = size_of::<Name>()
                    .checked_add(name.len())
                    .ok_or(RunError::Contract("type variable name copy quote overflow"))?;
                self.endpoint.admit_work(work)?;
                self.endpoint
                    .admit(ExecutionWork::Resource { requested_bytes })?;
                self.endpoint.check_completion()?;
                Ok(name.clone())
            })
            .await;
        Ok(self
            .endpoint
            .intern_value(
                &self.routes.typevar_identity_values,
                (name, definition, kind),
            )
            .await)
    }

    async fn intern_typevar_instance(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 4, 0, || (identity, bounds, variance, default)).await?;
        receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.intern_value(&self.routes.typevar_instance_values, fields).await)
        }).await
    }

    async fn intern_bound_typevar(
        &self,
        typevar: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 2, 0, || (typevar, identity)).await?;
        receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.intern_value(&self.routes.bound_typevar_values, fields).await)
        }).await
    }

    async fn intern_typevar_constraints(
        &self,
        elements: Box<[Type<'db>]>,
    ) -> RunResult<crate::types::typevar::TypeVarConstraints<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 2, 0, || (elements,)).await?;
        let constraints = receiver_constraint_child_at(&self.endpoint, || async {
            #[cfg(test)]
            crate::types::call::bind::constructor_matching::observations::observe_before_matching(
                crate::types::call::bind::constructor_matching::ConstructorMatchingOperation::ConstraintIntern,
            );
            Ok(self
                .endpoint
                .intern_value(&self.routes.typevar_constraints_values, fields)
                .await)
        })
        .await?;
        #[cfg(test)]
        crate::types::call::bind::constructor_matching::observations::observe_after_matching(
            crate::types::call::bind::constructor_matching::ConstructorMatchingOperation::ConstraintIntern,
        );
        Ok(constraints)
    }

    async fn bound_typevar_default(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let bytes = size_of::<Option<Type<'db>>>().checked_mul(3)
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Option<Type<'db>>>>().checked_mul(2)?))
            .ok_or(RunError::Contract("bound default result quotation overflow"))?;
        let key = local_with_fixed_transfers_at(&self.endpoint, 4, bytes, || variable.as_id()).await?;
        let value = receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.child_call(|| async {
                self.endpoint.fetch_ref(&self.routes.bound_typevar_default, key)?.await
            }).await)
        }).await?;
        // The initial admission funds this fixed copy and return after the canonical child.
        Ok(*value)
    }

    async fn lazy_typevar_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                self.endpoint.check_completion()?;
                Ok(variable.as_id())
            })
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.lazy_typevar_default, key)?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn intern_generic_context(
        &self,
        program: Program<'db>,
        variables: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 4, 0, || {
                if program != self.routes.program {
                    return Err(RunError::Contract("generic context program is foreign"));
                }
                Ok((program, variables))
        }).await??;
        let context = receiver_constraint_child_at(&self.endpoint, || async {
            #[cfg(test)]
            crate::types::call::bind::constructor_matching::observations::observe_before_matching(
                crate::types::call::bind::constructor_matching::ConstructorMatchingOperation::GenericContextIntern,
            );
            Ok(self.endpoint.intern_value(&self.routes.generic_context_values, fields).await)
        }).await?;
        #[cfg(test)]
        crate::types::call::bind::constructor_matching::observations::observe_after_matching(
            crate::types::call::bind::constructor_matching::ConstructorMatchingOperation::GenericContextIntern,
        );
        Ok(context)
    }

    async fn intern_typevar_set(
        &self,
        variables: TypeVarSetVariables<'db>,
    ) -> RunResult<TypeVarSetInner<'db>> {
        let effects = local_with_fixed_transfers_at(&self.endpoint, 2, 0, || {
            SourceEffects::new(self, self.routes.program)
        })
        .await?;
        let fields = effects.local_quoted_with_fixed_transfers(
            Ok((3, size_of::<(TypeVarSetVariables<'db>,)>())),
            || (variables,),
        )
        .await?;
        let inner = self
            .endpoint
            .intern_value(&self.routes.typevar_set_values, fields)
            .await;
        effects.local_with_fixed_transfers(1, 0, || inner).await
    }

    async fn tuple_class(&self, tuple: TupleType<'db>) -> RunResult<ClassType<'db>> {
        let program = self
            .endpoint
            .read_field(
                tuple
                    .field_requests(self.endpoint.field_request_context())
                    .program(),
                &FixedFieldCopy,
            )
            .await;
        let key = local_with_fixed_transfers_at(&self.endpoint, 3, 0, || {
                if program != self.routes.program {
                    return Err(RunError::Contract("tuple class program is foreign"));
                }
                Ok(tuple.as_id())
            })
            .await??;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.tuple_class, key)?
                    .await
            })
            .await;
        local_with_fixed_transfers_at(&self.endpoint, 1, 0, || *value).await
    }

    async fn intern_specialization(
        &self,
        context: GenericContext<'db>,
        types: Box<[Type<'db>]>,
        materialization: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>> {
        let read = boxed_future_with_fixed_transfers_at(
            &self.endpoint,
            generated_field_quote(
                |context: GenericContext<'db>, fields| context.field_requests(fields),
                |context: GenericContext<'db>, fields| context.field_requests(fields).program(),
            ),
            || {
                let request = context.field_requests(self.endpoint.field_request_context()).program();
                self.endpoint.read_field(request, &FixedFieldCopy)
            },
        ).await?;
        let program = read.await;
        let fields = local_with_fixed_transfers_at(&self.endpoint, 9, 0, || {
                if program != self.routes.program {
                    return Err(RunError::Contract(
                        "specialization context program is foreign",
                    ));
                }
                Ok((context, types, materialization, tuple))
        }).await??;
        receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.intern_value(&self.routes.specialization_values, fields).await)
        }).await
    }

    async fn intern_explicit_any_class(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<ExplicitAnyInstanceClass<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 1, 0, || (class,)).await?;
        receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.intern_value(&self.routes.explicit_any_values, fields).await)
        }).await
    }

    async fn intern_generic_alias(
        &self,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericAlias<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 3, 0, || (origin, specialization)).await?;
        receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.intern_value(&self.routes.generic_alias_values, fields).await)
        }).await
    }

    /// Interns owned tuple fields in their canonical program after admitting their transfers.
    async fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        let fields = local_with_fixed_transfers_at(&self.endpoint, 8, 0, || {
            if program != self.routes.program {
                return Err(RunError::Contract("tuple program is foreign"));
            }
            Ok((program, spec))
        })
        .await??;
        let future = boxed_future_with_fixed_transfers_at(&self.endpoint, Ok((0, 0)), || {
            self.endpoint.intern_value(&self.values.tuple, fields)
        })
        .await?;
        Ok(future.await)
    }

    async fn string_literal(&self, value: &str) -> RunResult<Type<'db>> {
        Ok(intern_member_name_literal(&self.endpoint, &self.values.string_literal, value).await)
    }

    async fn union_from_two_elements(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let key = self
            .endpoint
            .intern_value(&self.values.type_pair, (self.routes.program, first, second))
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.pair_union, key.as_id())?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn intersection_from_two_elements(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let key = self
            .endpoint
            .intern_value(&self.values.type_pair, (self.routes.program, first, second))
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.pair_intersection, key.as_id())?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn canonical_redundancy(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        let key = self
            .endpoint
            .intern_value(&self.values.type_pair, (self.routes.program, first, second))
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.pair_redundancy, key.as_id())?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn canonical_owned_assignability(
        &self,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>> {
        let effects = local_with_fixed_transfers_at(&self.endpoint, 2, 0, || {
            SourceEffects::new(self, self.routes.program)
        })
        .await?;
        let fields = effects
            .local_with_fixed_transfers(4, 0, || {
                if program != self.routes.program {
                    return Err(RunError::Contract("owned assignability program is foreign"));
                }
                Ok((program, source, target))
            })
            .await??;
        let key = effects
            .receiver_constraint_child(|| async {
                Ok(self.endpoint.intern_value(&self.values.type_pair, fields).await)
            })
            .await?;
        let value = effects
            .receiver_constraint_child(|| async {
                Ok(self
                    .endpoint
                    .child_call(|| async {
                        self.endpoint
                            .fetch_ref(&self.routes.pair_owned_assignability, key.as_id())?
                            .await
                    })
                    .await)
            })
            .await?;
        effects.local_with_fixed_transfers(2, 0, || value).await
    }

    async fn canonical_intersection_simplification(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification> {
        let pair = self
            .endpoint
            .intern_value(&self.values.type_pair, (self.routes.program, first, second))
            .await;
        let key = self
            .endpoint
            .intern_query_key(
                &self.routes.intersection_simplification_keys,
                (pair, polarity),
            )
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.intersection_simplification, key)?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn data_descriptor(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
        any_of_union: bool,
    ) -> RunResult<bool> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("data descriptor program is foreign"));
                }
                Ok(())
            })
            .await;
        let fields = SourceEffects::new(self, self.routes.program)
            .initialize_value(|| (ty, program, any_of_union))
            .await?;
        let key = self
            .endpoint
            .intern_query_key(&self.routes.data_descriptor_keys, fields)
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.data_descriptor, key)?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*value)
            })
            .await)
    }

    async fn effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<Option<VariableKind>> {
        let effects = SourceEffects::new(self, self.routes.program);
        effects.check_override_class_program(class).await?;
        let quote = effects
            .initialize_value(EffectiveVariableKindProfile::input_conversion_quote)
            .await?;
        self.endpoint
            .local_call(|| {
                let work = quote.work.checked_add(quote.cleanup_work).ok_or(
                    RunError::Contract("effective variable kind input work overflow"),
                )?;
                self.endpoint.admit_work(work)?;
                self.endpoint.check_completion()
            })
            .await;
        let fields = effects.initialize_value(|| (class, name.clone())).await?;
        let key = self
            .endpoint
            .intern_query_key(&self.routes.effective_variable_kind_keys, fields)
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.effective_variable_kind, key)?
                    .await
            })
            .await;
        effects.initialize_value(|| *value).await
    }

    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<bool> {
        let effects = SourceEffects::new(self, self.routes.program);
        effects.check_override_scope_program(scope).await?;
        let fields = effects.initialize_value(|| (scope, symbol)).await?;
        let key = self
            .endpoint
            .intern_query_key(&self.routes.function_definition_keys, fields)
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.function_definition, key)?
                    .await
            })
            .await;
        effects.initialize_value(|| *value).await
    }

    async fn cached_materialization(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("materialization program is foreign"));
                }
                Ok(())
            })
            .await;
        let key = self
            .endpoint
            .intern_query_key(&self.routes.materialization_keys, (ty, program, kind))
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.materialization, key)?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn apply_specialization(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> RunResult<Type<'db>> {
        let key = self
            .endpoint
            .intern_query_key(
                &self.routes.specialization_keys,
                (ty, specialization, specialize_self_domain),
            )
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.specialization, key)?
                    .await
            })
            .await;
        Ok(*value)
    }

    async fn intern_typeform(&self, argument: Type<'db>) -> RunResult<Type<'db>> {
        let form = self
            .endpoint
            .intern_value(&self.routes.typeform_values, (argument,))
            .await;
        Ok(Type::TypeForm(form))
    }

    async fn intern_dataclass_transformer_params(
        &self,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> RunResult<DataclassTransformerParams<'db>> {
        let make_fields = move || (flags, field_specifiers);
        let fields = SourceEffects::new(self, self.routes.program)
            .local_quoted(dataclass_transformer_input_quote(&make_fields), make_fields)
            .await?;
        Ok(self
            .endpoint
            .intern_value(&self.routes.dataclass_transformer_values, fields)
            .await)
    }

    async fn intern_union(
        &self,
        elements: Box<[Type<'db>]>,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<UnionType<'db>> {
        Ok(self
            .endpoint
            .intern_value(&self.values.union, (elements, recursively_defined))
            .await)
    }

    async fn intern_intersection(
        &self,
        positive: FxOrderSet<Type<'db>>,
        negative: NegativeIntersectionElements<'db>,
    ) -> RunResult<IntersectionType<'db>> {
        Ok(self
            .endpoint
            .intern_value(&self.values.intersection, (positive, negative))
            .await)
    }

    async fn intern_enum_class(
        &self,
        class: ClassLiteral<'db>,
        members: Box<[(Name, Type<'db>)]>,
        aliases: Box<[(Name, Name)]>,
        aliases_are_known: bool,
        members_are_exhaustive: bool,
    ) -> RunResult<EnumClassLiteral<'db>> {
        Ok(self
            .endpoint
            .intern_value(
                &self.routes.enum_class_values,
                (
                    class,
                    members,
                    aliases,
                    aliases_are_known,
                    members_are_exhaustive,
                ),
            )
            .await)
    }

    async fn class_literal(
        &self,
        identity: ClassIdentity<'_, 'db>,
    ) -> RunResult<StaticClassLiteral<'db>> {
        let fields = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok((
                    identity.name.clone(),
                    identity.body_scope,
                    identity.known,
                    identity.deprecated,
                    identity.type_check_only,
                    identity.dataclass_params,
                    identity.dataclass_transformer_params,
                    identity.total_ordering,
                    identity.has_decorators,
                    identity.has_type_params,
                    identity.has_explicit_bases,
                    identity.has_explicit_metaclass,
                ))
            })
            .await;
        Ok(self.endpoint.intern_value(&self.values.class, fields).await)
    }

    async fn function_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        let updated = function.read_updated_signature(&self.endpoint).await;
        if let Some(updated) = updated {
            return Ok(updated);
        }
        Ok(self
            .endpoint
            .child_call(|| async {
                let demand = self.endpoint.fetch_ref(&self.routes.signature, function.as_id())?;
                #[cfg(test)]
                let demand = tests::constructor_matching::self_receivers::specialization::observe_signature_request(function.as_id(), demand);
                demand.await
            })
            .await)
    }

    async fn function_last_definition_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db Signature<'db>> {
        let effects = last_signature_local(&self.endpoint, Some(1), Some(0), || {
            SourceEffects::new(self, self.routes.program)
        })
        .await?;
        Ok(effects
            .last_signature_future(|| {
                self.endpoint.child_call(|| async {
                    self.endpoint
                        .fetch_ref(&self.routes.last_definition_signature, function.as_id())?
                        .await
                })
            })
            .await?
            .await)
    }

    async fn known_class_lookup(
        &self,
        program: Program<'db>,
        class: KnownClass,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("known class program is foreign"));
                }
                Ok(())
            })
            .await;
        let argument = self
            .endpoint
            .intern_value(&self.values.known_class, (class, program))
            .await;
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.known_class, argument.as_id())?
                    .await
            })
            .await;
        Ok(*result)
    }

    async fn known_class_instance(
        &self,
        program: Program<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("known class program is foreign"));
                }
                Ok(())
            })
            .await;
        let argument = self
            .endpoint
            .intern_value(&self.values.known_class, (class, program))
            .await;
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.known_class_instance, argument.as_id())?
                    .await
            })
            .await;
        Ok(*result)
    }

    async fn class_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("class context program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.class_generic_context, key)?
                    .await
            })
            .await;
        Ok(*result)
    }

    async fn pep695_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let effects = local_with_fixed_transfers_at(&self.endpoint, 3, 0, || {
            SourceEffects::new(self, self.routes.program)
        }).await?;
        let file = receiver_constraint_child_at(&self.endpoint, || effects.static_class_file(class)).await?;
        receiver_constraint_child_at(&self.endpoint, || effects.check_file_program(file)).await?;
        let key = local_with_fixed_transfers_at(&self.endpoint, 1, 0, || class.as_id()).await?;
        let context = receiver_constraint_child_at(&self.endpoint, || async {
            Ok(self.endpoint.child_call(|| async {
                self.endpoint.fetch_ref(&self.routes.pep695_class_context, key)?.await
            }).await)
        }).await?;
        local_with_fixed_transfers_at(&self.endpoint, 1, 0, || *context).await
    }

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &FixedFieldCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<salsa::Id>(),
                })?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("code generator program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let generator = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.code_generator, key)?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<Option<CodeGeneratorKind<'db>>>() * 2,
                })?;
                self.endpoint.check_completion()?;
                Ok(*generator)
            })
            .await)
    }

    async fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db InstanceLayout> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("instance layout program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.instance_layout, key)?
                    .await
            })
            .await)
    }

    async fn abstract_methods(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<&'db FxIndexMap<Name, AbstractMethod<'db>>> {
        let literal = match class {
            ClassType::NonGeneric(literal) => literal,
            ClassType::Generic(alias) => ClassLiteral::Static(
                self.endpoint
                    .read_field(
                        alias
                            .field_requests(self.endpoint.field_request_context())
                            .origin(),
                        &BorrowOrCopy,
                    )
                    .await,
            ),
        };
        let file = SourceEffects::new(self, self.routes.program)
            .class_file(literal)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("abstract methods program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.abstract_methods, key)?
                    .await
            })
            .await)
    }

    async fn might_be_explicitly_abstract(&self, definition: Definition<'db>) -> RunResult<bool> {
        let file = SourceEffects::new(self, self.routes.program)
            .definition_file(definition)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("abstract candidate program is foreign"));
                }
                Ok(definition.as_id())
            })
            .await;
        let value = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.might_be_explicitly_abstract, key)?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(*value)
            })
            .await)
    }

    async fn implicit_attribute_names(&self, scope: ScopeId<'db>) -> RunResult<&'db [Name]> {
        let file = SourceEffects::new(self, self.routes.program)
            .scope_file(scope)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(3)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract(
                        "implicit attribute names program is foreign",
                    ));
                }
                Ok(scope.as_id())
            })
            .await;
        let names = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.implicit_attribute_names, key)?
                    .await
            })
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(names.as_ref())
            })
            .await)
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("explicit bases program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let bases = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.explicit_bases, key)?
                    .await
            })
            .await;
        Ok(bases.as_ref())
    }

    async fn inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>> {
        let effects = SourceEffects::new(self, self.routes.program);
        effects.check_inheritance_cycle_program(class).await?;
        let key = effects.initialize_value(|| class.as_id()).await?;
        let cycle = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.inheritance_cycle, key)?
                    .await
            })
            .await;
        effects.initialize_value(|| *cycle).await
    }

    async fn inner_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<crate::types::class::metaclass_selection::MetaclassSelectionResult<'db>> {
        let effects = SourceEffects::new(self, self.routes.program);
        effects.check_metaclass_program(class).await?;
        let key = effects.initialize_value(|| class.as_id()).await?;
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.inner_metaclass, key)?
                    .await
            })
            .await;
        effects.clone_metaclass_result(result).await
    }

    async fn class_decorators(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("class decorators program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let decorators = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.class_decorators, key)?
                    .await
            })
            .await;
        Ok(decorators.as_ref())
    }

    async fn inherited_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract(
                        "inherited class context program is foreign",
                    ));
                }
                Ok(class.as_id())
            })
            .await;
        let context = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.inherited_class_context, key)?
                    .await
            })
            .await;
        Ok(*context)
    }

    async fn inherited_instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("instance flags program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let flags = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.inherited_instance_flags, key)?
                    .await
            })
            .await;
        Ok(*flags)
    }

    async fn enum_metadata(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>> {
        self.enum_class_metadata(ClassLiteral::Static(class)).await
    }

    async fn enum_class_metadata(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("enum metadata program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let metadata = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.enum_metadata, key)?
                    .await
            })
            .await;
        Ok(metadata.as_ref())
    }

    async fn enum_class_literal(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("enum class program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        let metadata = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.enum_class_literal, key)?
                    .await
            })
            .await;
        Ok(*metadata)
    }

    async fn class_mro_literals(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<&'db [ClassLiteral<'db>]> {
        let effects = SourceEffects::new(self, self.routes.program);
        let file = effects.class_file(class).await?;
        let request = local_with_fixed_transfers_at(&self.endpoint, 16, 0, || {
            file.read_fields(self.endpoint.field_request_context()).program()
        }).await?;
        let program = self.endpoint.read_field(request, &BorrowOrCopy).await;
        let key = local_with_fixed_transfers_at(&self.endpoint, 4, 0, || {
            if program != self.routes.program {
                return Err(RunError::Contract("class MRO literals program is foreign"));
            }
            Ok(class.as_id())
        }).await??;
        let literals = self.endpoint.child_call(|| async {
            self.endpoint.fetch_ref(&self.routes.class_mro_literals, key)?.await
        }).await;
        local_with_fixed_transfers_at(&self.endpoint, 1, 0, || &**literals).await
    }

    async fn static_mro(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(class)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("static MRO program is foreign"));
                }
                Ok(class.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async { self.endpoint.fetch_ref(&self.routes.static_mro, key)?.await })
            .await)
    }

    async fn source_alias_mro(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        let origin = self
            .endpoint
            .read_field(
                alias
                    .field_requests(self.endpoint.field_request_context())
                    .origin(),
                &BorrowOrCopy,
            )
            .await;
        let file = SourceEffects::new(self, self.routes.program)
            .static_class_file(origin)
            .await?;
        let program = self
            .endpoint
            .read_field(
                file.read_fields(self.endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let key = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(4)?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("source alias MRO program is foreign"));
                }
                Ok(alias.as_id())
            })
            .await;
        Ok(self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.source_alias_mro, key)?
                    .await
            })
            .await)
    }

    async fn callable_type(
        &self,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<CallableType<'db>> {
        let signature = self
            .endpoint
            .local_call(|| {
                self.endpoint
                    .admit_work(signatures.overloads.len().saturating_add(1))?;
                let work = signatures.retirement_work().ok_or(RunError::Contract(
                    "callable signature retirement quotation overflow",
                ))?;
                let requested_bytes =
                    signatures
                        .clone_requested_bytes()
                        .ok_or(RunError::Contract(
                            "callable signature clone quotation overflow",
                        ))?;
                self.endpoint.admit_work(work)?;
                self.endpoint
                    .admit(salsa::execution_probe::ExecutionWork::Resource { requested_bytes })?;
                self.endpoint.check_completion()?;
                Ok(signatures.clone())
            })
            .await;
        self.owned_callable_type(signature, kind, None).await
    }

    async fn owned_callable_type(
        &self,
        signatures: CallableSignature<'db>,
        kind: CallableTypeKind,
        deprecated: Option<OverloadLiteral<'db>>,
    ) -> RunResult<CallableType<'db>> {
        Ok(self
            .endpoint
            .intern_value(&self.values.callable, (signatures, kind, deprecated))
            .await)
    }

    async fn intern_bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        class_method: bool,
        receiver: BoundMethodReceiver<'db>,
    ) -> RunResult<BoundMethodType<'db>> {
        let fields = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<(
                        Type<'db>,
                        Program<'db>,
                        bool,
                        BoundMethodReceiver<'db>,
                    )>(),
                })?;
                self.endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("bound method program is foreign"));
                }
                Ok((func, program, class_method, receiver))
            })
            .await;
        #[cfg(test)]
        tests::constructor_preparation::observe_before(
            tests::constructor_preparation::Stage::NativeBinding,
        );
        Ok(self
            .endpoint
            .intern_value(&self.values.bound_method, fields)
            .await)
    }

    async fn intern_descriptor_get_call_context(
        &self,
        descriptor_type: Type<'db>,
        callable_type: Type<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> RunResult<DescriptorGetCallContext<'db>> {
        let fields = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<(
                        Type<'db>,
                        Type<'db>,
                        Option<Type<'db>>,
                        Type<'db>,
                    )>(),
                })?;
                self.endpoint.check_completion()?;
                Ok((descriptor_type, callable_type, instance, owner))
            })
            .await;
        Ok(self
            .endpoint
            .intern_value(&self.values.descriptor_get_call_context, fields)
            .await)
    }

    async fn descriptor_dispatch(
        &self,
        signatures: CallableSignature<'db>,
        arguments: Box<[Type<'db>]>,
        comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
        selected_overloads: Box<[usize]>,
        failed: bool,
    ) -> RunResult<DescriptorDispatch<'db>> {
        let fields = SourceEffects::new(self, self.routes.program)
            .initialize_value(move || {
                (signatures, arguments, comparisons, selected_overloads, failed)
            })
            .await?;
        Ok(self
            .endpoint
            .intern_value(&self.values.descriptor_dispatch, fields)
            .await)
    }

    async fn descriptor_dispatches(
        &self,
        elements: Box<[DescriptorDispatch<'db>]>,
    ) -> RunResult<DescriptorDispatches<'db>> {
        let fields = SourceEffects::new(self, self.routes.program)
            .initialize_value(move || (elements,))
            .await?;
        Ok(self
            .endpoint
            .intern_value(&self.values.descriptor_dispatches, fields)
            .await)
    }

    async fn intern_property(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
        instance_class: PropertyInstanceClass<'db>,
        accessor_definitions: PropertyAccessorDefinitions<'db>,
    ) -> RunResult<PropertyInstanceType<'db>> {
        Ok(self
            .endpoint
            .intern_value(
                &self.values.property,
                (
                    getter,
                    setter,
                    deleter,
                    instance_class,
                    accessor_definitions,
                ),
            )
            .await)
    }

    async fn non_terminal_call(&self, call: CallableAndCallExpr<'db>) -> RunResult<Truthiness> {
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .fetch_ref(&self.routes.non_terminal_call, call.as_id())?
                    .await
            })
            .await;
        Ok(*result)
    }

    async fn unavailable<T>(&self, operation: SourceOperation) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.session
                    .unavailable(&self.endpoint, source_operation(operation))
            })
            .await)
    }

    async fn prepare_existing(&self, file: ProgramFile<'db>) -> RunResult<PreparedSource<'db>> {
        let fields = file.read_fields(self.endpoint.field_request_context());
        let python_file = self
            .endpoint
            .read_field(fields.python_file(), &BorrowOrCopy)
            .await;
        let file = self
            .endpoint
            .read_field(
                python_file
                    .read_fields(self.endpoint.field_request_context())
                    .file(),
                &BorrowOrCopy,
            )
            .await;
        let program = self
            .endpoint
            .read_field(fields.program(), &BorrowOrCopy)
            .await;
        self.prepare_file(file, program).await
    }

    async fn prepare_file(
        &self,
        file: File,
        program: Program<'db>,
    ) -> RunResult<PreparedSource<'db>> {
        let source = self.session.source(&self.endpoint, file, program).await?;
        source.read_source(&self.endpoint).await;
        Ok(self
            .endpoint
            .local_call(|| {
                // The catalog retains each module through every attempt and its cleanup.
                // Cloning these handles only increments the retained Arcs' reference counts.
                self.endpoint.admit_work(8)?;
                self.endpoint.check_completion()?;
                Ok(PreparedSource {
                    file: source.program_file(),
                    module: source.parsed_module().clone(),
                    index: source.semantic_index(),
                })
            })
            .await)
    }

    async fn resolve_module(
        &self,
        program: Program<'db>,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> RunResult<Option<Module<'db>>> {
        self.session
            .resolve_module(&self.endpoint, program, name, importing_file)
            .await
    }

    async fn known_module(&self, file: ProgramFile<'db>) -> RunResult<Option<KnownModule>> {
        self.session.read_known_module(&self.endpoint, file).await
    }

    async fn global_scope(&self, file: ProgramFile<'db>) -> RunResult<ScopeId<'db>> {
        self.session.read_global_scope(&self.endpoint, file).await
    }

    async fn file_module(&self, file: ProgramFile<'db>) -> RunResult<Option<Module<'db>>> {
        self.session.read_file_module(&self.endpoint, file).await
    }

    async fn module_literal(
        &self,
        module: Module<'db>,
        importing_file: Option<ProgramFile<'db>>,
    ) -> RunResult<ModuleLiteralType<'db>> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(7)?;
                self.endpoint.check_completion()?;
                Ok(())
            })
            .await;
        let environment = module.resolver_environment_with(&self.endpoint).await?;
        let program_environment = self
            .endpoint
            .read_field(
                self.routes
                    .program
                    .field_requests(self.endpoint.field_request_context())
                    .resolver_environment(),
                &BorrowOrCopy,
            )
            .await;
        self.endpoint
            .local_call(|| {
                if environment != program_environment {
                    return Err(RunError::Contract(
                        "module literal resolver environment is foreign",
                    ));
                }
                Ok(())
            })
            .await;
        if let Some(importing_file) = importing_file {
            let program = self
                .endpoint
                .read_field(
                    importing_file
                        .read_fields(self.endpoint.field_request_context())
                        .program(),
                    &BorrowOrCopy,
                )
                .await;
            self.endpoint
                .local_call(|| {
                    if program != self.routes.program {
                        return Err(RunError::Contract(
                            "module literal importing program is foreign",
                        ));
                    }
                    Ok(())
                })
                .await;
        }
        let kind = module.kind_with(&self.endpoint).await?;
        self.endpoint
            .local_call(|| {
                if importing_file.is_some() != kind.is_package() {
                    return Err(RunError::Contract(
                        "module literal importing file does not match module kind",
                    ));
                }
                Ok(())
            })
            .await;
        Ok(self
            .endpoint
            .intern_value(&self.values.module, (module, importing_file))
            .await)
    }
}

fn source_operation(operation: SourceOperation) -> OperationId {
    match operation {
        SourceOperation::StatementQuery => OperationId::StatementQuery,
        SourceOperation::StatementConditionDiagnostic => OperationId::StatementConditionDiagnostic,
        SourceOperation::StatementTry => OperationId::StatementTry,
        SourceOperation::StatementWith => OperationId::StatementWith,
        SourceOperation::StatementMatch => OperationId::StatementMatch,
        SourceOperation::StatementAssignment => OperationId::StatementAssignment,
        SourceOperation::StatementAnnotatedAssignment => OperationId::StatementAnnotatedAssignment,
        SourceOperation::StatementAugmentedAssignment => OperationId::StatementAugmentedAssignment,
        SourceOperation::StatementTypeAlias => OperationId::StatementTypeAlias,
        SourceOperation::StatementFor => OperationId::StatementFor,
        SourceOperation::StatementWhile => OperationId::StatementWhile,
        SourceOperation::StatementImport => OperationId::StatementImport,
        SourceOperation::StatementImportFrom => OperationId::StatementImportFrom,
        SourceOperation::StatementAssert => OperationId::StatementAssert,
        SourceOperation::StatementRaise => OperationId::StatementRaise,
        SourceOperation::StatementReturn => OperationId::StatementReturn,
        SourceOperation::StatementDelete => OperationId::StatementDelete,
        SourceOperation::StatementGlobal => OperationId::StatementGlobal,
        SourceOperation::ScopeBody => OperationId::ScopeBody,
        SourceOperation::ParameterAnnotation => OperationId::ParameterAnnotation,
        SourceOperation::ParameterDefault => OperationId::ParameterDefault,
        SourceOperation::TypingSelfBodyScope => OperationId::TypingSelfBodyScope,
        SourceOperation::BindingOwner => OperationId::BindingOwner,
        SourceOperation::BindingStorage => OperationId::BindingStorage,
        SourceOperation::BindingDiagnostic => OperationId::BindingDiagnostic,
        SourceOperation::BindingMember => OperationId::BindingMember,
        SourceOperation::AssignmentValidation(operation) => {
            OperationId::AssignmentValidation(operation)
        }
        SourceOperation::AssignmentUnpack => OperationId::AssignmentUnpack,
        SourceOperation::AssignmentCall => OperationId::AssignmentCall,
        SourceOperation::AssignmentNamedTuple => OperationId::AssignmentNamedTuple,
        SourceOperation::AssignmentTypedDict => OperationId::AssignmentTypedDict,
        SourceOperation::AssignmentNewClass => OperationId::AssignmentNewClass,
        SourceOperation::AssignmentEnum => OperationId::AssignmentEnum,
        SourceOperation::AssignmentParamSpec => OperationId::AssignmentParamSpec,
        SourceOperation::AssignmentTypeVarTuple => OperationId::AssignmentTypeVarTuple,
        SourceOperation::AssignmentNewType => OperationId::AssignmentNewType,
        SourceOperation::AssignmentBuiltinType => OperationId::AssignmentBuiltinType,
        SourceOperation::AssignmentTypeAliasType => OperationId::AssignmentTypeAliasType,
        SourceOperation::AssignmentSentinel => OperationId::AssignmentSentinel,
        SourceOperation::AssignmentDesugaredDecorator => OperationId::AssignmentDesugaredDecorator,
        SourceOperation::AssignmentDiagnostic => OperationId::AssignmentDiagnostic,
        SourceOperation::AnnotatedAssignment(operation) => {
            OperationId::AnnotatedAssignment(operation)
        }
        SourceOperation::LegacyTypeVarDiagnostic => OperationId::LegacyTypeVarDiagnostic,
        SourceOperation::LegacyTypeVarForwardedOwner => OperationId::LegacyTypeVarForwardedOwner,
        SourceOperation::TypeVarDefault(operation) => OperationId::TypeVarDefault(operation),
        SourceOperation::BoundTypeVarDefaultCycleNormalization => {
            OperationId::BoundTypeVarDefaultCycleNormalization
        }
        SourceOperation::BoundTypeVarDefaultRecursiveNormalization => {
            OperationId::BoundTypeVarDefaultRecursiveNormalization
        }
        SourceOperation::GenericAliasCycleMerge => OperationId::GenericAliasCycleMerge,
        SourceOperation::LegacyTypeVariables(operation) => {
            OperationId::LegacyTypeVariables(operation)
        }
        SourceOperation::DefaultSpecialization(operation) => {
            OperationId::DefaultSpecialization(operation)
        }
        SourceOperation::FunctionParamSpec => OperationId::FunctionParamSpec,
        SourceOperation::FunctionUnpackedKwargs => OperationId::FunctionUnpackedKwargs,
        SourceOperation::FunctionReturnCheck => OperationId::FunctionReturnCheck,
        SourceOperation::ReturnGeneratorType => OperationId::ReturnGeneratorType,
        SourceOperation::ReturnNoneType => OperationId::ReturnNoneType,

        SourceOperation::ScopeDeferred => OperationId::ScopeDeferred,
        SourceOperation::ScopePostcheck => OperationId::ScopePostcheck,
        SourceOperation::ClassCheck(operation) => OperationId::ClassCheck(operation),
        SourceOperation::OwnMember(operation) => OperationId::OwnMember(operation),
        SourceOperation::Definition(operation) => OperationId::DefinitionBody(operation),
        SourceOperation::Deferred(operation) => OperationId::Deferred(operation),
        SourceOperation::Expression(operation) => match operation {
            SourceExpressionOperation::ExpressionCache => OperationId::ExpressionCache,
            SourceExpressionOperation::ExpressionKind => OperationId::ExpressionKind,
            SourceExpressionOperation::StringLiteralExpectedType => {
                OperationId::StringLiteralExpectedType
            }
            SourceExpressionOperation::StringTypeAlias => OperationId::StringTypeAlias,
            SourceExpressionOperation::Call => OperationId::CallArguments,
            SourceExpressionOperation::ContextualTypeForm => OperationId::ContextualExpression,
            SourceExpressionOperation::ContextualClassSpecialization => {
                OperationId::ContextualExpression
            }
            SourceExpressionOperation::ContextualExpressionFinish => {
                OperationId::ContextualExpression
            }
            SourceExpressionOperation::Narrowing => OperationId::Narrowing,
            SourceExpressionOperation::RevealTypeFallback => OperationId::RevealType,
            SourceExpressionOperation::UnresolvedReference => OperationId::UnresolvedReference,
            SourceExpressionOperation::PossiblyUnresolvedReference => {
                OperationId::UnresolvedReference
            }
            SourceExpressionOperation::DeprecationDiagnostic => OperationId::Deprecation,
            SourceExpressionOperation::CallableDeprecation => OperationId::Deprecation,
        },
        SourceOperation::ExpressionScope => OperationId::ExpressionScope,
        SourceOperation::SuiteAwaitableNominal => OperationId::SuiteAwaitableNominal,
        SourceOperation::SuiteAwaitableUnion => OperationId::SuiteAwaitableUnion,
        SourceOperation::SuiteAwaitableIntersection => OperationId::SuiteAwaitableIntersection,
        SourceOperation::SuiteAwaitableKnownFunction => OperationId::SuiteAwaitableKnownFunction,
        SourceOperation::SuiteAwaitableDiagnostic => OperationId::SuiteAwaitableDiagnostic,
        SourceOperation::SuiteRedundantIf => OperationId::SuiteRedundantIf,
        SourceOperation::SuiteRedundantAssert => OperationId::SuiteRedundantAssert,
        SourceOperation::SuiteRedundantWhile => OperationId::SuiteRedundantWhile,
        SourceOperation::SuiteRedundantMatch => OperationId::SuiteRedundantMatch,
        SourceOperation::ContextualExpression => OperationId::ContextualExpression,
        SourceOperation::DataclassFieldSpecifiers => OperationId::DataclassFieldSpecifiers,
        SourceOperation::AnnotationString => OperationId::AnnotationString,
        SourceOperation::QuotedAnnotation(operation) => OperationId::QuotedAnnotation(operation),
        SourceOperation::AnnotationQualifier => OperationId::AnnotationQualifier,
        SourceOperation::AnnotationConditionalAlias => OperationId::AnnotationConditionalAlias,
        SourceOperation::TypeExpressionLegacy => OperationId::TypeExpressionLegacy,
        SourceOperation::TypeExpressionUnionRuntimeValidation => {
            OperationId::TypeExpressionUnionRuntimeValidation
        }
        SourceOperation::TypeExpressionSubscript => OperationId::TypeExpressionSubscript,
        SourceOperation::TypeExpressionCallableUnpack => OperationId::TypeExpressionCallableUnpack,
        SourceOperation::TypeExpressionRecursiveAlias => OperationId::TypeExpressionRecursiveAlias,
        SourceOperation::TypeExpressionParamSpecAttribute => {
            OperationId::TypeExpressionParamSpecAttribute
        }
        SourceOperation::TypeExpressionInvalid => OperationId::TypeExpressionInvalid,
        SourceOperation::TypeExpressionInitTypeVariableReport => {
            OperationId::TypeExpressionInitTypeVariableReport
        }
        SourceOperation::TypeExpressionAliasTypeVariableReport => {
            OperationId::TypeExpressionAliasTypeVariableReport
        }
        SourceOperation::TypeExpressionUnboundTypeVariableReport => {
            OperationId::TypeExpressionUnboundTypeVariableReport
        }
        SourceOperation::TypeExpressionSubclassArgument => {
            OperationId::TypeExpressionSubclassArgument
        }
        SourceOperation::TypeConversion(operation) => OperationId::TypeConversion(operation),
        SourceOperation::SubscriptExpectedKeys => OperationId::SubscriptExpectedKeys,
        SourceOperation::SubscriptImplicitAlias => OperationId::SubscriptImplicitAlias,
        SourceOperation::SubscriptReceiver => OperationId::SubscriptReceiver,
        SourceOperation::SubscriptExpressionTypes => OperationId::SubscriptExpressionTypes,
        SourceOperation::SubscriptDiagnostic => OperationId::SubscriptDiagnostic,
        SourceOperation::SubscriptLegacyArgumentTraversal => {
            OperationId::SubscriptLegacyArgumentTraversal
        }
        SourceOperation::ExplicitSpecializationProtocolMember => {
            OperationId::ExplicitSpecializationProtocolMember
        }
        SourceOperation::ExplicitSpecializationParamSpec => {
            OperationId::ExplicitSpecializationParamSpec
        }
        SourceOperation::ExplicitSpecializationVariadic => {
            OperationId::ExplicitSpecializationVariadic
        }
        SourceOperation::ExplicitSpecializationLazyUpperBound => {
            OperationId::ExplicitSpecializationLazyUpperBound
        }
        SourceOperation::ExplicitSpecializationLazyConstraints => {
            OperationId::ExplicitSpecializationLazyConstraints
        }
        SourceOperation::ExplicitSpecializationBounds => OperationId::ExplicitSpecializationBounds,
        SourceOperation::ExplicitSpecializationBoundMapping => {
            OperationId::ExplicitSpecializationBoundMapping
        }
        SourceOperation::ExplicitSpecializationDiagnostic => {
            OperationId::ExplicitSpecializationDiagnostic
        }
        SourceOperation::ExplicitSpecializationTupleClass => {
            OperationId::ExplicitSpecializationTupleClass
        }
        SourceOperation::ExplicitSpecializationTypeClass => {
            OperationId::ExplicitSpecializationTypeClass
        }
        SourceOperation::FunctionDecorator => OperationId::FunctionDecorator,
        SourceOperation::DecoratorApplication(operation) => {
            OperationId::DecoratorApplication(operation)
        }
        SourceOperation::FunctionMetadata => OperationId::FunctionMetadata,
        SourceOperation::FunctionTypeParameterShadowDiagnostic => {
            OperationId::FunctionTypeParameterShadowDiagnostic
        }
        SourceOperation::TypeParameterConstraintCountDiagnostic => {
            OperationId::TypeParameterConstraintCountDiagnostic
        }
        SourceOperation::ImportPolicy => OperationId::ImportPolicy,
        SourceOperation::ImportDiagnostic => OperationId::ImportDiagnostic,
        SourceOperation::ImportBinding => OperationId::ImportBinding,
        SourceOperation::Submodule => OperationId::Submodule,
        SourceOperation::ModuleGetattr => OperationId::ModuleGetattr,
        SourceOperation::ModuleTypeMember => OperationId::ModuleTypeMember,
        SourceOperation::ImplicitPlace => OperationId::ImplicitPlace,
        SourceOperation::ImplicitName(operation) => OperationId::ImplicitName(operation),
        SourceOperation::PlaceScope => OperationId::PlaceScope,
        SourceOperation::PlacePromotion => OperationId::PlacePromotion,
        SourceOperation::DiscardedBinding => OperationId::DiscardedBinding,
        SourceOperation::LoopHeader => OperationId::LoopHeader,
        SourceOperation::Reachability => OperationId::Reachability,
        SourceOperation::ReachabilityPrefix => OperationId::ReachabilityPrefix,
        SourceOperation::ReachabilityCheckpoint => OperationId::ReachabilityCheckpoint,
        SourceOperation::ReachabilityPredicate => OperationId::ReachabilityPredicate,
        SourceOperation::Truthiness(operation) => OperationId::Truthiness(operation),
        SourceOperation::ChainedComparison(operation) => OperationId::ChainedComparison(operation),
        SourceOperation::KnownClassInstance(operation) => {
            OperationId::KnownClassInstance(operation)
        }
        SourceOperation::InstanceFlagsMetaclass => OperationId::InstanceFlagsMetaclass,
        SourceOperation::ExplicitAnyInstanceConstruction => {
            OperationId::ExplicitAnyInstanceConstruction
        }
        SourceOperation::TypeComparison(operation) => OperationId::TypeComparison(operation),
        SourceOperation::TupleSpec(operation) => OperationId::TupleSpec(operation),
        SourceOperation::RecursiveNormalization(operation) => OperationId::RecursiveNormalization(operation),
        SourceOperation::Equality(operation) => OperationId::Equality(operation),
        SourceOperation::Attribute(operation) => OperationId::Attribute(operation),
        SourceOperation::MemberLookup(operation) => OperationId::MemberLookup(operation),
        SourceOperation::Descriptor(operation) => OperationId::Descriptor(operation),
        SourceOperation::CallableConversion(operation) => {
            OperationId::CallableConversion(operation)
        }
        SourceOperation::TypeSearch(operation) => OperationId::TypeSearch(operation),
        SourceOperation::Narrowing => OperationId::Narrowing,
        SourceOperation::Union => OperationId::Union,
        SourceOperation::FunctionComparison => OperationId::FunctionComparison,
        SourceOperation::Equivalence => OperationId::Equivalence,
        SourceOperation::MissingBinding => OperationId::MissingBinding,
        SourceOperation::CanonicalMerge => OperationId::CanonicalMerge,
        SourceOperation::ExpressionStorage => OperationId::ExpressionStorage,
        SourceOperation::ExpressionCache => OperationId::ExpressionCache,
        SourceOperation::CallArguments => OperationId::CallArguments,
        SourceOperation::CallBindings => OperationId::CallBindings,
        SourceOperation::CallableGuard(operation) => OperationId::CallableGuard(operation),
        SourceOperation::ConstructorPreparation(operation) => OperationId::ConstructorPreparation(operation),
        SourceOperation::ConstructorStorage(operation) => OperationId::ConstructorStorage(operation),
        SourceOperation::ConstructorSignature(operation) => OperationId::ConstructorSignature(operation),
        SourceOperation::BoundMethodPreparation(operation) => OperationId::BoundMethodPreparation(operation),
        SourceOperation::CallParameterMatching => OperationId::CallParameterMatching,
        SourceOperation::CallGenericFreshening => OperationId::CallGenericFreshening,
        SourceOperation::CallConstructorMatching => OperationId::CallConstructorMatching,
        SourceOperation::CallVariadicMatching => OperationId::CallVariadicMatching,
        SourceOperation::CallKeywordMatching => OperationId::CallKeywordMatching,
        SourceOperation::CallUnpackedMatching => OperationId::CallUnpackedMatching,
        SourceOperation::ArgumentPreparation => OperationId::ArgumentPreparation,
        SourceOperation::ArgumentCandidates => OperationId::ArgumentCandidates,
        SourceOperation::ArgumentGenericContext => OperationId::ArgumentGenericContext,
        SourceOperation::ArgumentNarrowing => OperationId::ArgumentNarrowing,
        SourceOperation::ArgumentSpeculation => OperationId::ArgumentSpeculation,
        SourceOperation::ArgumentTypeContext => OperationId::ArgumentTypeContext,
        SourceOperation::ArgumentChecking => OperationId::ArgumentChecking,
        SourceOperation::CheckerArgumentExpansion => OperationId::CheckerArgumentExpansion,
        SourceOperation::CheckerGenericInference => OperationId::CheckerGenericInference,
        SourceOperation::CheckerKnownFunction => OperationId::CheckerKnownFunction,
        SourceOperation::CheckerOverloadFiltering => OperationId::CheckerOverloadFiltering,
        SourceOperation::CheckerParameterUnion => OperationId::CheckerParameterUnion,
        SourceOperation::CheckerParamSpec => OperationId::CheckerParamSpec,
        SourceOperation::CheckerSpecialization => OperationId::CheckerSpecialization,
        SourceOperation::CheckerSplat => OperationId::CheckerSplat,
        SourceOperation::CheckerConstructor => OperationId::CheckerConstructor,
        SourceOperation::CheckerDownstreamConstructor => OperationId::CheckerDownstreamConstructor,
        SourceOperation::CheckerEquivalent => OperationId::CheckerEquivalent,
        SourceOperation::CheckerClassInfo => OperationId::CheckerClassInfo,
        SourceOperation::CheckerTypeVarTuple => OperationId::CheckerTypeVarTuple,
        SourceOperation::CheckerConstructorReceiver => OperationId::CheckerConstructorReceiver,
        SourceOperation::CallConstructorReturn => OperationId::CallConstructorReturn,
        SourceOperation::CallDeprecationBoundMethodType => {
            OperationId::CallDeprecationBoundMethodType
        }
        SourceOperation::CallDeprecationBoundMethodFunction => {
            OperationId::CallDeprecationBoundMethodFunction
        }
        SourceOperation::CallDeprecationDownstreamConstructor => {
            OperationId::CallDeprecationDownstreamConstructor
        }
        SourceOperation::CallDeprecationDiagnostic => OperationId::CallDeprecationDiagnostic,
        SourceOperation::CallDiscardedExtraArguments => OperationId::CallDiscardedExtraArguments,
        SourceOperation::CallKnownFunctionCheck => OperationId::CallKnownFunctionCheck,
        SourceOperation::CallKnownClassCheck => OperationId::CallKnownClassCheck,
        SourceOperation::CallNeverReveal => OperationId::CallNeverReveal,
        SourceOperation::CallReceiverConstraints => OperationId::CallReceiverConstraints,
        SourceOperation::CallRangeInference => OperationId::CallRangeInference,
        SourceOperation::CallTypeGuardBinding => OperationId::CallTypeGuardBinding,
        SourceOperation::CallDiagnostic => OperationId::CallDiagnostic,
        SourceOperation::CallCollectionReturn => OperationId::CallCollectionReturn,
        SourceOperation::CallReturnUnionBuilder => OperationId::CallReturnUnionBuilder,
        SourceOperation::CallReturnIntersectionBuilder => {
            OperationId::CallReturnIntersectionBuilder
        }
        SourceOperation::Relation(operation) => OperationId::Relation(operation),
        SourceOperation::Materialization(operation) => OperationId::Materialization(operation),
        SourceOperation::PublicPromotion(operation) => OperationId::PublicPromotion(operation),
            SourceOperation::SelfMapping(operation) => OperationId::SelfMapping(operation),
        SourceOperation::FreshenBoundTypeVars(operation) => OperationId::FreshenBoundTypeVars(operation),
        SourceOperation::ReturnCallableMapping(operation) => OperationId::ReturnCallableMapping(operation),
        SourceOperation::Specialization(operation) => OperationId::Specialization(operation),
        SourceOperation::LegacyTypeVarBinding(operation) => {
            OperationId::LegacyTypeVarBinding(operation)
        }
        SourceOperation::TypeAliasResolution => OperationId::TypeAliasResolution,
        SourceOperation::TypeVarBounds => OperationId::TypeVarBounds,
        SourceOperation::TypeVarBindingFunctionContext => {
            OperationId::TypeVarBindingFunctionContext
        }
        SourceOperation::TypeVarBindingCapturedParamSpec => {
            OperationId::TypeVarBindingCapturedParamSpec
        }
        SourceOperation::TypeVarBindingAliasContext => OperationId::TypeVarBindingAliasContext,
        SourceOperation::TypeVarDomainTop => OperationId::TypeVarDomainTop,
        SourceOperation::TypeVarConstraintUnion => OperationId::TypeVarConstraintUnion,
        SourceOperation::NewTypeBase => OperationId::NewTypeBase,
        SourceOperation::EnumMemberValue => OperationId::EnumMemberValue,
        SourceOperation::EnumComplementIntern => OperationId::EnumComplementIntern,
        SourceOperation::EnumComplementLiteralUnion => OperationId::EnumComplementLiteralUnion,
        SourceOperation::ClassSelection => OperationId::ClassSelection,
        SourceOperation::GenericIntersection => OperationId::GenericIntersection,
        SourceOperation::RecursiveTypeUnfold => OperationId::RecursiveTypeUnfold,
        SourceOperation::ContextualTypeFormPositive => OperationId::ContextualTypeFormPositive,
        SourceOperation::ContextualTypeFormFallback => OperationId::ContextualTypeFormFallback,
        SourceOperation::ContextualLiteralFilter => OperationId::ContextualLiteralFilter,
        SourceOperation::ContextualLiteralAssignability => {
            OperationId::ContextualLiteralAssignability
        }
        SourceOperation::CollectionConstraintStorage => OperationId::CollectionConstraintStorage,
        SourceOperation::CallMetadata => OperationId::CallMetadata,
        SourceOperation::CallSpecial => OperationId::CallSpecial,
        SourceOperation::SignatureTypeParameters => OperationId::SignatureTypeParameters,
        SourceOperation::SignatureClassReceiver => OperationId::SignatureClassReceiver,
        SourceOperation::SignatureAsyncReturn => OperationId::SignatureAsyncReturn,
        SourceOperation::SignatureParameterNormalization => {
            OperationId::SignatureParameterNormalization
        }
        SourceOperation::SignatureGenericContext => OperationId::SignatureGenericContext,
        SourceOperation::SignatureReturnCallables => OperationId::SignatureReturnCallables,
        SourceOperation::PostCheckDecoratorCalls => OperationId::PostCheckDecoratorCalls,
        SourceOperation::PostCheckFunctionLegacyPositional => {
            OperationId::PostCheckFunctionLegacyPositional
        }
        SourceOperation::PostCheckFunctionPep695 => OperationId::PostCheckFunctionPep695,
        SourceOperation::PostCheckFunctionTypeVarDefaults => {
            OperationId::PostCheckFunctionTypeVarDefaults
        }
        SourceOperation::PostCheckFunctionTypeVarOrdering => {
            OperationId::PostCheckFunctionTypeVarOrdering
        }
        SourceOperation::PostCheckOverloads => OperationId::PostCheckOverloads,
        SourceOperation::PostCheckTypeGuard => OperationId::PostCheckTypeGuard,
        SourceOperation::PostCheckFinalValue => OperationId::PostCheckFinalValue,
        SourceOperation::PostCheckDynamicClass => OperationId::PostCheckDynamicClass,
        SourceOperation::FileImplicitAlias => OperationId::FileImplicitAlias,
        SourceOperation::FileReadError => OperationId::FileReadError,
        SourceOperation::FileParseDiagnostic => OperationId::FileParseDiagnostic,
        SourceOperation::FileUnsupportedSyntaxDiagnostic => {
            OperationId::FileUnsupportedSyntaxDiagnostic
        }
        SourceOperation::FileSemanticSyntaxDiagnostic => OperationId::FileSemanticSyntaxDiagnostic,
        SourceOperation::FileDiagnosticSort => OperationId::FileDiagnosticSort,
        SourceOperation::FileSuppressionSelection => OperationId::FileSuppressionSelection,
        SourceOperation::FileSuppressionUnknownRule => OperationId::FileSuppressionUnknownRule,
        SourceOperation::FileSuppressionInvalid => OperationId::FileSuppressionInvalid,
        SourceOperation::FileSuppressionBlanket => OperationId::FileSuppressionBlanket,
        SourceOperation::FileSuppressionUnused => OperationId::FileSuppressionUnused,
        SourceOperation::Finalization => OperationId::Finalization,
    }
}

fn register<'run, 'session, 'db: 'run, Resources>(
    session: &'run AnalysisSession<'session, 'db>,
    prepared: &'run PreparedAnalysisFile<'db>,
    mut registry: RegistryBuilder<'run, 'db>,
    values: &'run SourceValues<'db>,
    resources: Resources,
) -> RunResult<(
    RegisteredRun<'run, 'db>,
    Rc<SourceRoutes<'run, 'db, Resources>>,
)>
where
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
{
    let db = session.db();
    if !prepared.belongs_to(db) {
        return Err(RunError::Contract(
            "source preparation belongs to a different database state",
        ));
    }
    registry.enable_structural_dependency_validation()?;
    let scope = registry.reserve_callable(db, scope_inference_ingredient(db))?;
    let expression = registry.reserve_callable(db, expression_inference_ingredient(db))?;
    let definition = registry.reserve_callable(db, definition_inference_ingredient(db))?;
    let signature = registry.reserve_callable(db, function_literal_signature_ingredient(db))?;
    let last_definition_signature =
        registry.reserve_callable(db, function_last_definition_signature_ingredient(db))?;
    let non_terminal_call = registry.reserve_callable(db, non_terminal_call_ingredient(db))?;
    let known_class = registry.reserve_callable(db, known_class_to_class_literal_ingredient(db))?;
    let known_class_instance =
        registry.reserve_callable(db, known_class_to_instance_ingredient(db))?;
    let class_generic_context =
        registry.reserve_callable(db, static_class_generic_context_ingredient(db))?;
    let pep695_class_context = registry.reserve_callable(db, pep695_generic_context_ingredient(db))?;
    let enum_metadata = registry.reserve_callable(db, enum_metadata_ingredient(db))?;
    let enum_class_literal = registry.reserve_callable(db, enum_class_literal_ingredient(db))?;
    let enum_class_values = register_enum_class_values(db, &mut registry)?;
    let static_mro = registry.reserve_callable(db, try_mro_unspecialized_ingredient(db))?;
    let class_mro_literals = registry.reserve_callable(db, class_mro_literals_ingredient(db))?;
    let source_alias_mro = registry.reserve_callable(db, source_alias_mro_ingredient(db))?;
    let deferred_definition =
        registry.reserve_callable(db, deferred_definition_inference_ingredient(db))?;
    let function_decorators =
        registry.reserve_callable(db, function_decorator_inference_ingredient(db))?;
    let function_overloads =
        registry.reserve_callable(db, overloads_and_implementation_ingredient(db))?;
    let explicit_bases = registry.reserve_callable(db, explicit_bases_ingredient(db))?;
    let inheritance_cycle =
        registry.reserve_callable(db, inheritance_cycle_inner_ingredient(db))?;
    let inner_metaclass = registry.reserve_callable(db, try_metaclass_inner_ingredient(db))?;
    let class_decorators = registry.reserve_callable(db, class_decorators_ingredient(db))?;
    let inherited_instance_flags =
        registry.reserve_callable(db, instance_flags_inner_ingredient(db))?;
    let bound_typevar_default =
        registry.reserve_callable(db, bound_typevar_default_ingredient(db))?;
    let lazy_typevar_default =
        registry.reserve_callable(db, lazy_typevar_default_ingredient(db))?;
    let inherited_class_context =
        registry.reserve_callable(db, inherited_class_context_ingredient(db))?;
    let runtime_visibility = registry.reserve_callable(db, runtime_visibility_ingredient(db))?;
    let member = registry.reserve_callable(db, member_lookup_ingredient(db))?;
    let class_member = registry.reserve_callable(db, class_member_lookup_ingredient(db))?;
    let constructor_new = registry.reserve_callable(db, LookupDunderNewQuery::ingredient(db))?;
    let constructor_new_keys =
        registry.callable_query_keys::<_, ConstructorNewKeyProfile>(&constructor_new)?;
    let code_generator =
        registry.reserve_callable(db, code_generator_of_static_class_ingredient(db))?;
    let instance_layout = registry.reserve_callable(db, instance_layout_ingredient(db))?;
    let implicit_attribute_names =
        registry.reserve_callable(db, implicit_attribute_names_ingredient(db))?;
    let abstract_methods = registry.reserve_callable(db, abstract_methods_ingredient(db))?;
    let might_be_explicitly_abstract =
        registry.reserve_callable(db, might_be_explicitly_abstract_ingredient(db))?;
    let module_type_body_scope =
        registry.reserve_callable(db, module_type_body_scope_ingredient(db))?;
    let data_descriptor = registry.reserve_callable(db, data_descriptor_ingredient(db))?;
    let data_descriptor_keys =
        registry.callable_query_keys::<_, DataDescriptorKeyProfile>(&data_descriptor)?;
    let effective_variable_kind =
        registry.reserve_callable(db, effective_superclass_variable_kind_ingredient(db))?;
    let effective_variable_kind_keys = registry
        .callable_query_keys::<_, EffectiveVariableKindProfile>(&effective_variable_kind)?;
    let function_definition = registry.reserve_callable(db, is_function_definition_ingredient(db))?;
    let function_definition_keys =
        registry.callable_query_keys::<_, FunctionDefinitionProfile>(&function_definition)?;
    let member_metadata_values =
        registry.finite_interned_values_with_memos(MemberMetadata::ingredient(db.zalsa()), ())?;
    let member_error_values = registry
        .finite_interned_values_with_memos(MemberLookupError::ingredient(db.zalsa()), ())?;
    let place_table = registry.reserve_callable(
        db as &dyn ty_python_core::Db,
        ty_python_core::finalized_sources::place_table_ingredient(db),
    )?;
    let use_def_map = registry.reserve_callable(
        db as &dyn ty_python_core::Db,
        ty_python_core::finalized_sources::use_def_map_ingredient(db),
    )?;
    let expression_narrowing =
        registry.reserve_callable(db, expression_narrowing_constraints_ingredient(db))?;
    let pair_union = registry.reserve_callable(db, union_from_two_elements_ingredient(db))?;
    let pair_intersection =
        registry.reserve_callable(db, intersection_from_two_elements_ingredient(db))?;
    let pair_redundancy =
        registry.reserve_callable(db, crate::types::relation::redundancy_ingredient(db))?;
    let pair_owned_assignability =
        registry.reserve_callable(db, crate::types::relation::owned_assignability_ingredient(db))?;
    let intersection_simplification =
        registry.reserve_callable(db, intersection_simplification_ingredient(db))?;
    let intersection_simplification_keys =
        registry.fixed_callable_query_keys(&intersection_simplification)?;
    let materialization = registry.reserve_callable(db, cached_materialization_ingredient(db))?;
    let materialization_keys =
        registry.callable_query_keys::<_, MaterializationKeyProfile>(&materialization)?;
    let specialization = registry.reserve_callable(db, apply_specialization_ingredient(db))?;
    let specialization_keys =
        registry.callable_query_keys::<_, SpecializationKeyProfile>(&specialization)?;
    let place = registry.reserve_callable(db, place_by_id_ingredient(db))?;
    let place_keys = registry.callable_query_keys::<_, PlaceLookupKeyProfile>(&place)?;
    let dunder_all = registry.reserve_callable(db, dunder_all_names_ingredient(db))?;
    let descriptor = registry.reserve_callable(db, descriptor_get_ingredient(db))?;
    let descriptor_keys =
        registry.callable_query_keys::<_, DescriptorLookupKeyProfile>(&descriptor)?;
    let typeform_values =
        registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
    let dataclass_transformer_values = register_dataclass_transformer_values(db, &mut registry)?;
    let typevar_identity_values = register_typevar_identity_values(db, &mut registry)?;
    let typevar_constraints_values = register_typevar_constraints_values(db, &mut registry)?;
    let typevar_instance_values = register_typevar_instance_values(db, &mut registry)?;
    let bound_typevar_values = register_bound_typevar_values(db, &mut registry)?;
    let generic_context_values = register_generic_context_values(db, &mut registry)?;
    let typevar_set_values = register_typevar_set_values(db, &mut registry)?;
    let tuple_class = registry.reserve_callable(db, crate::types::tuple::to_class_type_ingredient(db))?;
    let specialization_values = register_specialization_values(db, &mut registry)?;
    let generic_alias_values = register_generic_alias_values(db, &mut registry)?;
    let explicit_any_values = register_explicit_any_values(db, &mut registry)?;
    registry.bind_callable(&place_table, scope_maps::PlaceTableProvider { session })?;
    registry.bind_callable(&use_def_map, scope_maps::UseDefMapProvider { session })?;
    let routes = SourceRoutes::allocate(&registry, || {
        SourceRoutes {
        resources,
        program: session.program(),
        scope,
        expression,
        definition,
        signature,
        last_definition_signature,
        non_terminal_call,
        known_class,
        known_class_instance,
        class_generic_context,
        pep695_class_context,
        enum_metadata,
        enum_class_literal,
        enum_class_values,
        static_mro,
        class_mro_literals,
        source_alias_mro,
        deferred_definition,
        function_decorators,
        function_overloads,
        explicit_bases,
        inheritance_cycle,
        inner_metaclass,
        class_decorators,
        inherited_instance_flags,
        bound_typevar_default,
        lazy_typevar_default,
        inherited_class_context,
        runtime_visibility,
        member,
        member_metadata_values,
        member_error_values,
        place_table,
        use_def_map,
        expression_narrowing,
        pair_union,
        pair_intersection,
        pair_redundancy,
        pair_owned_assignability,
        intersection_simplification,
        intersection_simplification_keys,
        materialization,
        materialization_keys,
        specialization,
        specialization_keys,
        place,
        place_keys,
        dunder_all,
        descriptor,
        descriptor_keys,
        class_member,
        constructor_new,
        constructor_new_keys,
        code_generator,
        instance_layout,
        implicit_attribute_names,
        abstract_methods,
        might_be_explicitly_abstract,
        module_type_body_scope,
        data_descriptor,
        data_descriptor_keys,
        effective_variable_kind,
        effective_variable_kind_keys,
        function_definition,
        function_definition_keys,
        typeform_values,
        dataclass_transformer_values,
        typevar_identity_values,
        typevar_constraints_values,
        typevar_instance_values,
        bound_typevar_values,
        generic_context_values,
        typevar_set_values,
        tuple_class,
        specialization_values,
        generic_alias_values,
        explicit_any_values,
    }
    })?;
    registry.bind_callable(
        &routes.scope,
        ScopeProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.tuple_class,
        TupleClassProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.expression,
        ExpressionProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.definition,
        DefinitionProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.signature,
        FunctionSignatureProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.last_definition_signature,
        FunctionSignatureProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.non_terminal_call,
        NonTerminalCallProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.known_class,
        KnownClassProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.known_class_instance,
        KnownClassInstanceProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.class_generic_context,
        ClassGenericContextProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    let pep695_context_routes = routes.clone();
    registry.bind_callable(
        &routes.pep695_class_context,
        Pep695ClassContextProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: pep695_context_routes.clone(),
                values,
            },
        },
    )?;
    registry.bind_callable(
        &routes.enum_metadata,
        EnumMetadataProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.enum_class_literal,
        EnumClassProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.static_mro,
        StaticMroProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    let class_mro_literals_routes = routes.clone();
    registry.bind_callable(
        &routes.class_mro_literals,
        ClassMroLiteralsProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: class_mro_literals_routes.clone(),
                values,
            },
        },
    )?;
    let source_alias_mro_routes = routes.clone();
    registry.bind_callable(
        &routes.source_alias_mro,
        SourceAliasMroProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: source_alias_mro_routes.clone(),
                values,
            },
        },
    )?;
    registry.bind_callable(
        &routes.deferred_definition,
        DeferredDefinitionProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.explicit_bases,
        ExplicitBasesProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    let inner_metaclass_routes = routes.clone();
    registry.bind_callable(
        &routes.inner_metaclass,
        inner_metaclass::InnerMetaclassProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: inner_metaclass_routes.clone(),
                values,
            },
        },
    )?;
    let inheritance_cycle_routes = routes.clone();
    registry.bind_callable(
        &routes.inheritance_cycle,
        inheritance_cycle::InheritanceCycleProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: inheritance_cycle_routes.clone(),
                values,
            },
        },
    )?;
    registry.bind_callable(
        &routes.inherited_class_context,
        InheritedClassContextProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.runtime_visibility,
        RuntimeVisibilityProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.member,
        MemberLookupProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.expression_narrowing,
        ExpressionNarrowingProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.pair_union,
        TypePairProvider {
            kind: TypePairOperation::Union,
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.pair_intersection,
        TypePairProvider {
            kind: TypePairOperation::Intersection,
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.pair_redundancy,
        RedundancyProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.pair_owned_assignability,
        OwnedAssignabilityProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    registry.bind_callable(
        &routes.intersection_simplification,
        IntersectionSimplificationProvider {
            session,
            routes: routes.clone(),
            values,
        },
    )?;
    let function_decorator_routes = routes.clone();
    registry.bind_callable(
        &routes.function_decorators,
        FunctionDecoratorsProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: function_decorator_routes.clone(),
                values,
            },
        },
    )?;
    let overload_routes = routes.clone();
    let function_overloads_provider = FunctionOverloadsProvider {
        program: routes.program,
        access: move |endpoint| SourceQueryAccess {
            session,
            endpoint,
            routes: overload_routes.clone(),
            values,
        },
    };
    #[cfg(test)]
    let function_overloads_provider =
        tests::overload_collection::CycleProvider::new(function_overloads_provider);
    registry.bind_callable(&routes.function_overloads, function_overloads_provider)?;
    let decorator_routes = routes.clone();
    registry.bind_callable(
        &routes.class_decorators,
        ClassDecoratorsProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: decorator_routes.clone(),
                values,
            },
        },
    )?;
    let instance_flags_routes = routes.clone();
    registry.bind_callable(
        &routes.inherited_instance_flags,
        InstanceFlagsProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: instance_flags_routes.clone(),
                values,
            },
        },
    )?;
    let bound_default_routes = routes.clone();
    let bound_typevar_default_provider = BoundTypeVarDefaultProvider {
        program: routes.program,
        access: move |endpoint| SourceQueryAccess {
            session,
            endpoint,
            routes: bound_default_routes.clone(),
            values,
        },
    };
    #[cfg(test)]
    let bound_typevar_default_provider =
        tests::default_recovery::BoundRecoveryProvider::new(bound_typevar_default_provider);
    registry.bind_callable(
        &routes.bound_typevar_default,
        bound_typevar_default_provider,
    )?;
    let lazy_default_routes = routes.clone();
    let lazy_typevar_default_provider = LazyTypeVarDefaultProvider {
        program: routes.program,
        access: move |endpoint| SourceQueryAccess {
            session,
            endpoint,
            routes: lazy_default_routes.clone(),
            values,
        },
    };
    #[cfg(test)]
    let lazy_typevar_default_provider =
        tests::default_recovery::LazyRecoveryProvider::new(lazy_typevar_default_provider);
    registry.bind_callable(
        &routes.lazy_typevar_default,
        lazy_typevar_default_provider,
    )?;
    let materialization_routes = routes.clone();
    let materialization_provider = MaterializationProvider {
        access: move |endpoint| SourceQueryAccess {
            session,
            endpoint,
            routes: materialization_routes.clone(),
            values,
        },
    };
    #[cfg(test)]
    let materialization_provider =
        tests::union_recovery::RecoveryProvider::new(materialization_provider);
    registry.bind_callable(&routes.materialization, materialization_provider)?;
    let specialization_routes = routes.clone();
    let specialization_provider = SpecializationProvider {
        program: routes.program,
        access: move |endpoint| SourceQueryAccess {
            session,
            endpoint,
            routes: specialization_routes.clone(),
            values,
        },
    };
    #[cfg(test)]
    let specialization_provider =
        tests::specialization_recovery::RecoveryProvider::new(specialization_provider);
    registry.bind_callable(&routes.specialization, specialization_provider)?;
    let place_routes = routes.clone();
    registry.bind_callable(
        &routes.place,
        PlaceProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: place_routes.clone(),
                values,
            },
        },
    )?;
    let dunder_all_routes = routes.clone();
    registry.bind_callable(
        &routes.dunder_all,
        DunderAllProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: dunder_all_routes.clone(),
                values,
            },
        },
    )?;
    let descriptor_routes = routes.clone();
    registry.bind_callable(
        &routes.descriptor,
        descriptor::DescriptorProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: descriptor_routes.clone(),
                values,
            },
        },
    )?;
    let class_member_routes = routes.clone();
    registry.bind_callable(
        &routes.class_member,
        class_member::ClassMemberProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: class_member_routes.clone(),
                values,
            },
        },
    )?;
    let constructor_new_routes = routes.clone();
    registry.bind_callable(
        &routes.constructor_new,
        constructor_new::ConstructorNewProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: constructor_new_routes.clone(),
                values,
            },
        },
    )?;
    let code_generator_routes = routes.clone();
    registry.bind_callable(
        &routes.code_generator,
        code_generator::CodeGeneratorProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: code_generator_routes.clone(),
                values,
            },
        },
    )?;
    let instance_layout_routes = routes.clone();
    registry.bind_callable(
        &routes.instance_layout,
        instance_layout::InstanceLayoutProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: instance_layout_routes.clone(),
                values,
            },
        },
    )?;
    let implicit_names_routes = routes.clone();
    registry.bind_callable(
        &routes.implicit_attribute_names,
        implicit_names::ImplicitNamesProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: implicit_names_routes.clone(),
                values,
            },
        },
    )?;
    let abstract_methods_routes = routes.clone();
    registry.bind_callable(
        &routes.abstract_methods,
        abstract_methods::AbstractMethodsProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: abstract_methods_routes.clone(),
                values,
            },
        },
    )?;
    let might_be_explicitly_abstract_routes = routes.clone();
    registry.bind_callable(
        &routes.might_be_explicitly_abstract,
        abstract_methods::MightBeExplicitlyAbstractProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: might_be_explicitly_abstract_routes.clone(),
                values,
            },
        },
    )?;
    let module_type_scope_routes = routes.clone();
    registry.bind_callable(
        &routes.module_type_body_scope,
        module_type_scope::ModuleTypeScopeProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: module_type_scope_routes.clone(),
                values,
            },
        },
    )?;
    let data_descriptor_routes = routes.clone();
    registry.bind_callable(
        &routes.data_descriptor,
        data_descriptor::DataDescriptorProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: data_descriptor_routes.clone(),
                values,
            },
        },
    )?;
    let effective_variable_kind_routes = routes.clone();
    registry.bind_callable(
        &routes.effective_variable_kind,
        override_variable_kind::EffectiveVariableKindProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: effective_variable_kind_routes.clone(),
                values,
            },
        },
    )?;
    let function_definition_routes = routes.clone();
    registry.bind_callable(
        &routes.function_definition,
        override_variable_kind::FunctionDefinitionProvider {
            program: routes.program,
            access: move |endpoint| SourceQueryAccess {
                session,
                endpoint,
                routes: function_definition_routes.clone(),
                values,
            },
        },
    )?;
    Ok((registry.seal()?, routes))
}

struct DunderAllProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for DunderAllProvider<'db, MakeAccess>
where
    C: DunderAllConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<FxHashSet<Name>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let mut names = Some(
            SourceEffects::new(&access, self.program)
                .infer_dunder_all(file)
                .await?,
        );
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<Option<FxHashSet<Name>>>() * 2 + 2)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                tests::dunder_all::observe_completed(_db, file.as_id());
                names
                    .take()
                    .ok_or(RunError::Contract("dunder all result already consumed"))
            })
            .await)
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<FxHashSet<Name>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<Option<FxHashSet<Name>>>() * 2 + 1)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                tests::dunder_all::observe_initial(id);
                Ok(C::cycle_initial(db, id, file))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<FxHashSet<Name>>,
        value: Option<FxHashSet<Name>>,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<FxHashSet<Name>>>
    where
        'run: 'call,
    {
        let mut value = Some(value);
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<Option<FxHashSet<Name>>>() * 2 + 1)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                tests::dunder_all::observe_recovery(db, cycle.id(), cycle.iteration());
                let value = value
                    .take()
                    .ok_or(RunError::Contract("dunder all candidate already consumed"))?;
                let recovered = C::recover_from_cycle(db, cycle, last, value, file);
                #[cfg(test)]
                tests::dunder_all::observe_recovered(cycle.id());
                Ok(recovered)
            })
            .await)
    }
}

struct FunctionOverloadsProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for FunctionOverloadsProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = OverloadLiteral<'a>,
            Output<'a> = (Box<[OverloadLiteral<'a>]>, Option<OverloadLiteral<'a>>),
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_overloads(last_definition)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, last_definition))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
        value: (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)>
    where
        'run: 'call,
    {
        let mut value = Some(value);
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.check_completion()?;
                let value = value.take().ok_or(RunError::Contract(
                    "function overloads candidate already consumed",
                ))?;
                Ok(C::recover_from_cycle(
                    db,
                    cycle,
                    last,
                    value,
                    last_definition,
                ))
            })
            .await)
    }
}

struct FunctionDecoratorsProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for FunctionDecoratorsProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = FunctionDecoratorInference<'a>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecoratorInference<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_function_decorators(definition)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecoratorInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<FunctionDecoratorInference<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, definition))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call FunctionDecoratorInference<'db>,
        value: FunctionDecoratorInference<'db>,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecoratorInference<'db>>
    where
        'run: 'call,
    {
        // The generated recovery transfers the candidate; its producer already admitted cleanup.
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<FunctionDecoratorInference<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, definition))
            })
            .await)
    }
}

struct ClassDecoratorsProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for ClassDecoratorsProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_class_decorators(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, class))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Box<[Type<'db>]>,
        value: Box<[Type<'db>]>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}

struct InstanceFlagsProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for InstanceFlagsProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = ClassInstanceFlags,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_instance_flags(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, class))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call ClassInstanceFlags,
        value: ClassInstanceFlags,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}

struct BoundTypeVarDefaultProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for BoundTypeVarDefaultProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = BoundTypeVarInstance<'a>,
            Output<'a> = Option<Type<'a>>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_bound_typevar_default(variable)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, variable))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .recover_bound_typevar_default(cycle, last, value, variable)
            .await
    }
}

struct LazyTypeVarDefaultProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for LazyTypeVarDefaultProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypeVarInstance<'a>,
            Output<'a> = Option<Type<'a>>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_lazy_typevar_default(variable)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, variable))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .recover_lazy_typevar_default(cycle, last, value, variable)
            .await
    }
}

struct MaterializationProvider<MakeAccess> {
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for MaterializationProvider<MakeAccess>
where
    C: MaterializationConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // Generated tuple conversion copies Type's borrowed payload and two scalar fields.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 4,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "materialization input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                <Type<'db> as native_values::OutputProfile<'db>>::comparison_work(
                    endpoint, left, right,
                )
                .await?
            }
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (ty, program, kind): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, program)
            .materialize(ty, program, kind)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Type<'db>,
        value: Type<'db>,
        (_ty, program, _kind): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        SourceEffects::new(&access, program)
            .cycle_normalize(&env, value, *last, cycle)
            .await
    }
}

struct SpecializationProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for SpecializationProvider<'db, MakeAccess>
where
    C: SpecializationConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // Generated tuple conversion copies Type's borrowed payload and two scalar fields.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 4,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "specialization input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                <Type<'db> as native_values::OutputProfile<'db>>::comparison_work(
                    endpoint, left, right,
                )
                .await?
            }
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (ty, specialization, specialize_self_domain): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .apply_specialization_query(ty, specialization, specialize_self_domain)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Type<'db>,
        value: Type<'db>,
        (_ty, specialization, _specialize_self_domain): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        let context = SpecializationEffects::generic_context(&effects, db, specialization).await?;
        let program = SpecializationEffects::program(&effects, db, context).await?;
        let program = SpecializationEffects::environment(&effects, program).await?;
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        effects.cycle_normalize(&env, value, *last, cycle).await
    }
}

struct PlaceProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for PlaceProvider<'db, MakeAccess>
where
    C: PlaceConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        quote_place_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (scope, place, reexport, considered): C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_place_by_id(scope, place, reexport, considered)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                tests::canonical_place::observe_initial(id);
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call PlaceAndQualifiers<'db>,
        value: PlaceAndQualifiers<'db>,
        (scope, _place, _reexport, _considered): C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        let file = effects.scope_file(scope).await?;
        let program = endpoint
            .read_field(
                file.read_fields(endpoint.field_request_context()).program(),
                &BorrowOrCopy,
            )
            .await;
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 2)?;
                endpoint.check_completion()?;
                if program != self.program {
                    return Err(RunError::Contract("place recovery program is foreign"));
                }
                #[cfg(test)]
                tests::canonical_place::observe_recovery(_db, cycle.id(), cycle.iteration());
                Ok(ProgramEnvironment::from_scope(scope))
            })
            .await;
        let recovered = effects
            .normalize_place_cycle(&env, value, *last, cycle)
            .await?;
        #[cfg(test)]
        tests::canonical_place::observe_recovered(cycle.id());
        Ok(recovered)
    }
}

struct ClassMroLiteralsProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, A, MakeAccess>
    CallableRouteProvider<'run, 'db, crate::types::ClassMroLiteralsConfiguration>
    for ClassMroLiteralsProvider<'db, MakeAccess>
where
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, crate::types::ClassMroLiteralsConfiguration>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<Box<[ClassLiteral<'db>]>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program).infer_class_mro_literals(class).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Box<[ClassLiteral<'db>]>>
    where
        'run: 'call,
    {
        // The canonical query declares no cycle initializer; Salsa rejects cycles before this call.
        local_with_fixed_transfers_at(&endpoint, 1, 0, || {
            Err(RunError::Contract("class MRO literals has no cycle initializer"))
        }).await?
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Box<[ClassLiteral<'db>]>,
        value: Box<[ClassLiteral<'db>]>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Box<[ClassLiteral<'db>]>>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 1, 0, || {
            let _value = value;
            Err(RunError::Contract("class MRO literals has no cycle recovery"))
        }).await?
    }
}

struct SourceAliasMroProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for SourceAliasMroProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = GenericAlias<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let fields = alias.field_requests(endpoint.field_request_context());
        let origin = endpoint.read_field(fields.origin(), &BorrowOrCopy).await;
        let specialization = endpoint
            .read_field(fields.specialization(), &BorrowOrCopy)
            .await;
        SourceEffects::new(&access, self.program)
            .infer_static_mro(origin, Some(specialization))
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        alias: GenericAlias<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let fields = alias.field_requests(endpoint.field_request_context());
        let origin = endpoint.read_field(fields.origin(), &BorrowOrCopy).await;
        let specialization = endpoint
            .read_field(fields.specialization(), &BorrowOrCopy)
            .await;
        SourceEffects::new(&access, self.program)
            .initial_static_mro(origin, Some(specialization))
            .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Result<Mro<'db>, Box<StaticMroError<'db>>>,
        value: Result<Mro<'db>, Box<StaticMroError<'db>>>,
        _alias: GenericAlias<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let mut value = Some(value);
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(
                    size_of::<Option<Result<Mro<'db>, Box<StaticMroError<'db>>>>>() * 2 + 1,
                )?;
                endpoint.check_completion()?;
                value.take().ok_or(RunError::Contract(
                    "source alias MRO candidate already consumed",
                ))
            })
            .await)
    }
}

struct StaticMroProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::class::static_literal::TryMroUnspecializedConfiguration,
    > for StaticMroProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::static_literal::TryMroUnspecializedConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        #[cfg(test)]
        let _layout_child = tests::instance_layout::observe_mro_child(access.db());
        SourceEffects::new(&access, self.routes.program)
            .infer_static_mro(class, None)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .initial_static_mro(class, None)
            .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Result<Mro<'db>, Box<StaticMroError<'db>>>,
        value: Result<Mro<'db>, Box<StaticMroError<'db>>>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

struct EnumMetadataProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::enums::EnumMetadataConfiguration>
    for EnumMetadataProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, crate::types::enums::EnumMetadataConfiguration>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_enum_metadata(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(Some(EnumMetadata::empty()))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<EnumMetadata<'db>>,
        value: Option<EnumMetadata<'db>>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumMetadata<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

struct EnumClassProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::enums::EnumClassLiteralConfiguration>
    for EnumClassProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::enums::EnumClassLiteralConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_enum_class_literal(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(None)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<EnumClassLiteral<'db>>,
        value: Option<EnumClassLiteral<'db>>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

/// Evaluates the existing declaration-owned class-header query through shared source effects.
#[derive(Debug)]
struct Pep695ClassContextProvider<'db, MakeAccess> {
    program: Program<'db>,
    access: MakeAccess,
}

impl<'run, 'db: 'run, A, MakeAccess> CallableRouteProvider<
    'run,
    'db,
    crate::types::class::static_literal::Pep695GenericContextInnerConfiguration,
> for Pep695ClassContextProvider<'db, MakeAccess>
where
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, crate::types::class::static_literal::Pep695GenericContextInnerConfiguration>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 6, 0, || {
            match operation {
                NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<StaticClassLiteral<'db>>(),
                    cleanup_work: 0,
                }),
                NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => Err(RunError::Contract("class context input requires generated handle conversion")),
                NativeValueOperation::Comparison { .. } => Ok(NativeValueQuote {
                    work: 2,
                    requested_bytes: size_of::<bool>(),
                    cleanup_work: 0,
                }),
            }
        }).await?
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = local_with_fixed_transfers_at(&endpoint, 3, 0, || {
            SourceEffects::new(&access, self.program)
        }).await?;
        receiver_constraint_child_at(&endpoint, || effects.infer_pep695_class_context(class)).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 1, 0, || None).await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<GenericContext<'db>>,
        value: Option<GenericContext<'db>>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 1, 0, || value).await
    }
}

struct ClassGenericContextProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::class::static_literal::StaticClassGenericContextConfiguration,
    > for ClassGenericContextProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::static_literal::StaticClassGenericContextConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_class_generic_context(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(None)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<GenericContext<'db>>,
        value: Option<GenericContext<'db>>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

/// Runs the existing exact-tuple class query and its cycle initializer through source effects.
struct TupleClassProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> TupleClassProvider<'run, 'session, 'db, Resources>
{
    /// Admits access and effect carriers before running the shared tuple conversion.
    async fn run(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        tuple: TupleType<'db>,
        cycle: Option<salsa::Id>,
    ) -> RunResult<ClassType<'db>> {
        let access = local_quoted_with_fixed_transfers_at(
            &endpoint,
            Ok((12, size_of::<TupleType<'db>>() + size_of::<Option<salsa::Id>>())),
            || SourceQueryAccess {
                    session: self.session,
                    endpoint: endpoint.clone(),
                    routes: self.routes.clone(),
                    values: self.values,
                },
        )
        .await?;
        let effects = local_with_fixed_transfers_at(&endpoint, 2, 0, || {
            SourceEffects::new(&access, self.routes.program)
        })
        .await?;
        let class = effects.infer_tuple_class(tuple, cycle).await?;
        effects
            .local_with_fixed_transfers(1, 0, || class)
            .await
    }
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::tuple::ToClassTypeConfiguration>
    for TupleClassProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, crate::types::tuple::ToClassTypeConfiguration>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        crate::types::tuple::quote_class_conversion_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        tuple: TupleType<'db>,
    ) -> RunResult<ClassType<'db>>
    where
        'run: 'call,
    {
        self.run(endpoint, tuple, None).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        tuple: TupleType<'db>,
    ) -> RunResult<ClassType<'db>>
    where
        'run: 'call,
    {
        self.run(endpoint, tuple, Some(id)).await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call ClassType<'db>,
        value: ClassType<'db>,
        _tuple: TupleType<'db>,
    ) -> RunResult<ClassType<'db>>
    where
        'run: 'call,
    {
        local_with_fixed_transfers_at(&endpoint, 2, 0, || value).await
    }
}

struct KnownClassProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::class::KnownClassToClassLiteralConfiguration>
    for KnownClassProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::KnownClassToClassLiteralConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        argument: KnownClassArgument<'db>,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>>
    where
        'run: 'call,
    {
        let fields = argument.field_requests(endpoint.field_request_context());
        let program = endpoint.read_field(fields.program(), &BorrowOrCopy).await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("known class program is foreign"));
                }
                Ok(())
            })
            .await;
        let class = endpoint.read_field(fields.class(), &BorrowOrCopy).await;
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, program)
            .infer_known_class(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _argument: KnownClassArgument<'db>,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(Ok(None))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>,
        value: Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>,
        _argument: KnownClassArgument<'db>,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

struct KnownClassInstanceProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::class::KnownClassToInstanceConfiguration>
    for KnownClassInstanceProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::KnownClassToInstanceConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        argument: KnownClassArgument<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let fields = argument.field_requests(endpoint.field_request_context());
        let program = endpoint.read_field(fields.program(), &BorrowOrCopy).await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("known class program is foreign"));
                }
                Ok(())
            })
            .await;
        let class = endpoint.read_field(fields.class(), &BorrowOrCopy).await;
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, program)
            .infer_known_class_instance(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _argument: KnownClassArgument<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(Type::divergent(id))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Type<'db>,
        _value: Type<'db>,
        _argument: KnownClassArgument<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session.unavailable(
                    &endpoint,
                    OperationId::KnownClassInstance(
                        KnownClassInstanceOperation::CycleNormalization,
                    ),
                )
            })
            .await)
    }
}

struct FunctionSignatureProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::function::FunctionLiteralSignatureConfiguration>
    for FunctionSignatureProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::function::FunctionLiteralSignatureConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> RunResult<CallableSignature<'db>>
    where
        'run: 'call,
    {
        #[cfg(test)]
        tests::constructor_matching::self_receivers::specialization::signature_entered(function.as_id());
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_function_signature(function)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _function: FunctionType<'db>,
    ) -> RunResult<CallableSignature<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::FunctionSignatureCycleInitial)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call CallableSignature<'db>,
        _value: CallableSignature<'db>,
        _function: FunctionType<'db>,
    ) -> RunResult<CallableSignature<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::FunctionSignatureCycleRecovery)
            })
            .await)
    }
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::function::FunctionLastDefinitionSignatureConfiguration>
    for FunctionSignatureProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::function::FunctionLastDefinitionSignatureConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> RunResult<Signature<'db>>
    where
        'run: 'call,
    {
        let access = last_signature_local(&endpoint, Some(12), Some(0), || SourceQueryAccess {
            session: self.session,
            endpoint: endpoint.clone(),
            routes: self.routes.clone(),
            values: self.values,
        })
        .await?;
        let effects = last_signature_local(&endpoint, Some(1), Some(0), || {
            SourceEffects::new(&access, self.routes.program)
        })
        .await?;
        effects
            .last_signature_future(|| effects.infer_function_last_definition_signature(function))
            .await?
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        function: FunctionType<'db>,
    ) -> RunResult<Signature<'db>>
    where
        'run: 'call,
    {
        // Signature::bottom constructs two parameters, their shared storage and a signature.
        // The names are static. Storage work includes final-owner parameter retirement.
        let storage = parameters_storage_quote(2)
            .ok_or(RunError::Contract("bottom signature storage quotation overflow"))?;
        last_signature_local(
            &endpoint,
            storage.work.checked_add(24),
            storage.bytes.checked_add(size_of::<[Parameter<'db>; 2]>()),
            || {
                crate::types::function::FunctionLastDefinitionSignatureConfiguration::cycle_initial(
                    db, id, function,
                )
            },
        )
        .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Signature<'db>,
        value: Signature<'db>,
        function: FunctionType<'db>,
    ) -> RunResult<Signature<'db>>
    where
        'run: 'call,
    {
        // This query configures only cycle_initial. Its generated recovery returns the candidate
        // unchanged; the candidate's construction already paid for its eventual destruction.
        last_signature_local(
            &endpoint,
            Some(1),
            Some(size_of::<Option<Signature<'db>>>()),
            || (),
        )
        .await?;
        let mut value = Some(value);
        last_signature_local(
            &endpoint,
            Some(3),
            Some(0),
            || {
                let value = value
                    .take()
                    .ok_or(RunError::Contract("last signature candidate was consumed"))?;
                Ok(crate::types::function::FunctionLastDefinitionSignatureConfiguration::recover_from_cycle(
                    db, cycle, last, value, function,
                ))
            },
        )
        .await?
    }
}

struct NonTerminalCallProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::reachability::AnalyzeNonTerminalCallConfiguration>
    for NonTerminalCallProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::reachability::AnalyzeNonTerminalCallConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        call: CallableAndCallExpr<'db>,
    ) -> RunResult<Truthiness>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_non_terminal_call(call)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _call: CallableAndCallExpr<'db>,
    ) -> RunResult<Truthiness>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(non_terminal_call_initial())
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Truthiness,
        value: Truthiness,
        _call: CallableAndCallExpr<'db>,
    ) -> RunResult<Truthiness>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                Ok(non_terminal_call_recover(cycle, *last, value))
            })
            .await)
    }
}

struct ScopeProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::infer::InferScopeTypesImplConfiguration>
    for ScopeProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::infer::InferScopeTypesImplConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        input: InferScope<'db>,
    ) -> RunResult<ScopeInference<'db>>
    where
        'run: 'call,
    {
        let InferScope::Bare(scope) = input else {
            return Ok(endpoint
                .local_call(|| {
                    self.session
                        .unavailable(&endpoint, OperationId::ContextualScopeKey)
                })
                .await);
        };
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_scope(scope, TypeContext::default())
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: InferScope<'db>,
    ) -> RunResult<ScopeInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::ScopeCycleInitial)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call ScopeInference<'db>,
        _value: ScopeInference<'db>,
        _input: InferScope<'db>,
    ) -> RunResult<ScopeInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::ScopeCycleRecovery)
            })
            .await)
    }
}

struct ExpressionProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::infer::InferExpressionTypesImplConfiguration>
    for ExpressionProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::infer::InferExpressionTypesImplConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        input: InferExpression<'db>,
    ) -> RunResult<ExpressionInference<'db>>
    where
        'run: 'call,
    {
        let (expression, context) = match input {
            InferExpression::Bare(expression) => (expression, TypeContext::default()),
            InferExpression::WithContext(contextual) => {
                let fields = contextual.field_requests(endpoint.field_request_context());
                let expression = endpoint
                    .read_field(fields.expression(), &BorrowOrCopy)
                    .await;
                #[cfg(test)]
                tests::contextual_expression::observe(
                    tests::contextual_expression::Stage::ExpressionFieldRead,
                    self.session.db(),
                );
                let context = endpoint.read_field(fields.tcx(), &BorrowOrCopy).await;
                #[cfg(test)]
                tests::contextual_expression::observe(
                    tests::contextual_expression::Stage::ContextFieldRead,
                    self.session.db(),
                );
                (expression, context)
            }
        };
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        let effects = SourceEffects::new(&access, self.routes.program);
        let inference = effects.infer_expression(expression, context);
        #[cfg(test)]
        let inference = tests::contextual_tuple::observe_inference(inference);
        inference.await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: InferExpression<'db>,
    ) -> RunResult<ExpressionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::ExpressionCycleInitial)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call ExpressionInference<'db>,
        _value: ExpressionInference<'db>,
        _input: InferExpression<'db>,
    ) -> RunResult<ExpressionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::ExpressionCycleRecovery)
            })
            .await)
    }
}

struct DefinitionProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::infer::InferDefinitionTypesConfiguration>
    for DefinitionProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::infer::InferDefinitionTypesConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_definition(input)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::DefinitionCycleInitial)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call DefinitionInference<'db>,
        _value: DefinitionInference<'db>,
        _input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::DefinitionCycleRecovery)
            })
            .await)
    }
}

struct DeferredDefinitionProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::infer::InferDeferredTypesConfiguration>
    for DeferredDefinitionProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::infer::InferDeferredTypesConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_deferred_definition(input)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::DeferredDefinitionCycleInitial)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call DefinitionInference<'db>,
        _value: DefinitionInference<'db>,
        _input: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session
                    .unavailable(&endpoint, OperationId::DeferredDefinitionCycleRecovery)
            })
            .await)
    }
}

struct ExplicitBasesProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::class::static_literal::ExplicitBasesInnerConfiguration,
    > for ExplicitBasesProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::static_literal::ExplicitBasesInnerConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_explicit_bases(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .initial_explicit_bases(id, class)
            .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Box<[Type<'db>]>,
        value: Box<[Type<'db>]>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Box<[Type<'db>]>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .recover_explicit_bases(cycle, last, value, class)
            .await
    }
}

struct InheritedClassContextProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::class::static_literal::InheritedLegacyGenericContextInnerConfiguration,
    > for InheritedClassContextProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::class::static_literal::InheritedLegacyGenericContextInnerConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_inherited_class_context(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(None)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<GenericContext<'db>>,
        value: Option<GenericContext<'db>>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

struct RuntimeVisibilityProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::MayExistAtRuntimeConfiguration>
    for RuntimeVisibilityProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, crate::types::MayExistAtRuntimeConfiguration>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        input: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .infer_runtime_visibility(input)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _input: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session.unavailable(
                    &endpoint,
                    source_operation(SourceOperation::ImplicitName(
                        ImplicitNameOperation::RuntimeVisibilityCycle,
                    )),
                )
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.session.unavailable(
                    &endpoint,
                    source_operation(SourceOperation::ImplicitName(
                        ImplicitNameOperation::RuntimeVisibilityCycle,
                    )),
                )
            })
            .await)
    }
}

struct MemberLookupProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, crate::types::MemberLookupWithPolicyInnerConfiguration>
    for MemberLookupProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::MemberLookupWithPolicyInnerConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        key: MemberLookupKey<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        let program = endpoint
            .read_field(
                key.field_requests(endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("member lookup program is foreign"));
                }
                Ok(())
            })
            .await;
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        crate::types::member_lookup::general::member_lookup_dispatch_with(
            key,
            None,
            crate::types::member_lookup::general::GeneralMemberFacts,
            &SourceEffects::new(&access, self.routes.program),
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _key: MemberLookupKey<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                tests::nominal_members::observe_initial(id);
                Ok(crate::place::Place::bound(Type::divergent(id)).into())
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call MemberLookupResult<'db>,
        value: MemberLookupResult<'db>,
        key: MemberLookupKey<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        #[cfg(test)]
        tests::nominal_members::observe_recovery(_db, cycle.id(), cycle.iteration());
        let access = create_source_access(&endpoint, |endpoint| SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        })
        .await?;
        let program = endpoint
            .read_field(
                key.field_requests(endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 2)?;
                endpoint.check_completion()?;
                if program != self.routes.program {
                    return Err(RunError::Contract("member recovery program is foreign"));
                }
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        let result = SourceEffects::new(&access, self.routes.program)
            .normalize_member_cycle(&env, value, *last, cycle)
            .await?;
        #[cfg(test)]
        tests::nominal_members::observe_recovered(cycle.id());
        Ok(result)
    }
}

struct ExpressionNarrowingProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::narrow::AllNarrowingConstraintsForExpressionConfiguration,
    > for ExpressionNarrowingProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::narrow::AllNarrowingConstraintsForExpressionConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        expression: Expression<'db>,
    ) -> RunResult<ExpressionNarrowingConstraints<'db>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .expression_narrowing(expression)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _expression: Expression<'db>,
    ) -> RunResult<ExpressionNarrowingConstraints<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(ExpressionNarrowingConstraints::default())
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call ExpressionNarrowingConstraints<'db>,
        value: ExpressionNarrowingConstraints<'db>,
        _expression: Expression<'db>,
    ) -> RunResult<ExpressionNarrowingConstraints<'db>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()
            })
            .await;
        Ok(value)
    }
}

#[derive(Clone, Copy)]
enum TypePairOperation {
    Union,
    Intersection,
}

struct TypePairProvider<'run, 'session, 'db: 'run, Resources> {
    kind: TypePairOperation,
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    C,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, C> for TypePairProvider<'run, 'session, 'db, Resources>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = TypePair<'a>, Output<'a> = Type<'a>>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        pair: TypePair<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        let effects = SourceEffects::new(&access, self.routes.program);
        match self.kind {
            TypePairOperation::Union => effects.type_pair_union(pair).await,
            TypePairOperation::Intersection => effects.type_pair_intersection(pair).await,
        }
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _pair: TypePair<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(Type::divergent(id))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Type<'db>,
        _value: Type<'db>,
        _pair: TypePair<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                let operation = match self.kind {
                    TypePairOperation::Union => OperationId::Union,
                    TypePairOperation::Intersection => OperationId::Narrowing,
                };
                self.session.unavailable(&endpoint, operation)
            })
            .await)
    }
}

/// Runs the canonical lazy assignability query with the source run's retained relation owners.
struct OwnedAssignabilityProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>
    CallableRouteProvider<
        'run,
        'db,
        crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration,
    > for OwnedAssignabilityProvider<'run, 'session, 'db, Resources>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<
            'call,
            'db,
            crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration,
        >,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let quote = receiver_constraint_child_at(&endpoint, || {
            native_values::quote(endpoint.clone(), operation)
        })
        .await?;
        local_with_fixed_transfers_at(&endpoint, 2, 0, || quote).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        pair: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let access = local_with_fixed_transfers_at(&endpoint, 12, 0, || SourceQueryAccess {
            session: self.session,
            endpoint: endpoint.clone(),
            routes: self.routes.clone(),
            values: self.values,
        })
        .await?;
        let effects = local_with_fixed_transfers_at(&endpoint, 2, 0, || {
            SourceEffects::new(&access, self.routes.program)
        })
        .await?;
        let value = effects
            .receiver_constraint_child(|| effects.type_pair_owned_assignability(pair))
            .await?;
        effects.local_with_fixed_transfers(2, 0, || value).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        pair: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        // The generated initializer constructs an arena-free true terminal. Its one owning
        // handle needs one retirement visit in addition to construction and fixed transfers.
        local_with_fixed_transfers_at(&endpoint, 9, 0, || {
            crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration::cycle_initial(
                db, id, pair,
            )
        })
        .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call OwnedConstraintSet<'db>,
        value: OwnedConstraintSet<'db>,
        pair: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let retirement = local_with_fixed_transfers_at(&endpoint, 16, 0, || {
            value.retirement_work()
        })
        .await?;
        let quote = retirement
            .and_then(|work| work.checked_add(8))
            .map(|work| (work, 0))
            .ok_or(RunError::Contract("owned assignability recovery quotation overflow"));
        // Recovery returns the newly computed owner unchanged. Keep it outside the refusing
        // callback until admission succeeds, including when it retains a conditional arena.
        local_quoted_with_fixed_transfers_at(&endpoint, quote, || {
            crate::types::relation::WhenConstraintSetAssignableToOwnedImplConfiguration::recover_from_cycle(
                db, cycle, last, value, pair,
            )
        })
        .await
    }
}

struct RedundancyProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    C,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, C> for RedundancyProvider<'run, 'session, 'db, Resources>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = TypePair<'a>, Output<'a> = bool>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        pair: TypePair<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .type_pair_redundancy(pair)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _pair: TypePair<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(true)
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call bool,
        value: bool,
        _pair: TypePair<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(value)
            })
            .await)
    }
}

struct IntersectionSimplificationProvider<'run, 'session, 'db: 'run, Resources> {
    session: &'run AnalysisSession<'session, 'db>,
    routes: Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
}

impl<
    'run,
    'session,
    'db: 'run,
    C,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
> CallableRouteProvider<'run, 'db, C>
    for IntersectionSimplificationProvider<'run, 'session, 'db, Resources>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = (TypePair<'a>, IntersectionPolarity),
            Output<'a> = IntersectionSimplification,
        >,
{
    async fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // Generated input conversion copies an interned identity and a finite polarity.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 2,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "intersection simplification requires retained tuple fields",
                ));
            }
            NativeValueOperation::Comparison { .. } => 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (pair, polarity): (TypePair<'db>, IntersectionPolarity),
    ) -> RunResult<IntersectionSimplification>
    where
        'run: 'call,
    {
        let access = SourceQueryAccess {
            session: self.session,
            endpoint,
            routes: self.routes.clone(),
            values: self.values,
        };
        SourceEffects::new(&access, self.routes.program)
            .type_pair_simplification(pair, polarity)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: (TypePair<'db>, IntersectionPolarity),
    ) -> RunResult<IntersectionSimplification>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call IntersectionSimplification,
        _value: IntersectionSimplification,
        _input: (TypePair<'db>, IntersectionPolarity),
    ) -> RunResult<IntersectionSimplification>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Err(RunError::Contract(
                    "intersection simplification has immediate cycle fallback",
                ))
            })
            .await)
    }
}

impl<'db> native_values::OutputProfile<'db> for Option<EnumClassLiteral<'db>> {
    async fn comparison_work<'run>(
        _endpoint: TaskEndpoint<'run, 'db>,
        _left: &Self,
        _right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        Ok(2)
    }
}
