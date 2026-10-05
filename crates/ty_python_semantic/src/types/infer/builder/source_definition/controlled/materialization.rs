//! Mapping tasks retain source access independently of their callers.

use std::borrow::Cow;
use std::future::Future;
use std::hash::BuildHasherDefault;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use crate::{FxIndexMap, FxOrderSet};

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::storage::ordered_merge;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class_base::specialization::{
    ClassBaseMapping, ClassBaseSpecializationEffects, ClassBaseSpecializationWork,
};
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::function::{
    FunctionLiteral, FunctionType, OverloadLiteral, UpdatedFunctionSignatures,
};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::mapping::{CompositionStartEffects, compose_specialization_root_with};
use crate::types::generics::prefix::InitializedTypePrefix;
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::instance::{ExplicitAnyInstanceClass, NominalKnownClassEffects};
use crate::types::local_transfer::{generated_field_quote, local_quoted_with_fixed_transfers_at};
use crate::types::mapping::OwnedTypeMapping;
use crate::types::mapping::return_callables::{RetainedReturnCallables, RetainedReturnTypevars};
use crate::types::mapping::source::{
    FixedMappingField, MappingResourceAccess, MappingSourceEffects, RetainedMappingSource,
};
use crate::types::mapping::specialization::{SpecializationEffects, shared_specialization_with};
use crate::types::set_theoretic::builder::controlled_union::UnionEffects;
use crate::types::set_theoretic::builder::intersection_insertion::{Elements, InsertionEffects};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::set_theoretic::{
    IntersectionBuilder, IntersectionType, RecursivelyDefined, UnionBuilder, UnionType,
};
use crate::types::signatures::{CallableSignature, Parameter, ParameterKind, Parameters, ParametersKind, Signature};
use crate::types::tuple::construction::tuple_type;
use crate::types::tuple::buffer::TupleBufferStorageEffects;
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::typevar::{
    BoundTypeVarIdentity, TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints,
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::typevar::bounds::typevar_bounds_with;
use crate::types::visitor::runtime::TypeSliceDeref;
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassBase, ClassType, GenericAlias, GenericContext, KnownClass, KnownUnion, NominalInstanceType,
    MaterializationKind, MaterializationOperation, SelfBinding, Specialization, StaticClassLiteral, Type,
    TypeFormType,
};
use crate::{Db, Program, ProgramEnvironment};

mod storage;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleBufferStorageEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn new_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>> {
        self.function_mapping_child(|| self.new_mapping_tuple_elements(capacity)).await
    }

    async fn push_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.function_mapping_child(|| self.push_mapping_tuple_element(elements, ty)).await
    }

    async fn finish_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>> {
        self.function_mapping_child(|| self.finish_mapping_tuple_elements(elements, variable)).await
    }
}

#[derive(Debug)]
struct OwnedMappingSource<'db, A> {
    access: A,
    program: Program<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RetainedMappingSource<'run, 'db>
    for OwnedMappingSource<'db, A>
{
    type Effects<'call>
        = SourceEffects<'call, 'run, 'db, A>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_> {
        SourceEffects::new(&self.access, self.program)
    }

    async fn retained_clone(&self) -> RunResult<Self> {
        retained_mapping_source(&self.access, self.program).await
    }
}

/// Admits an access clone and its retained mapping wrapper before constructing either.
/// The wrapper owns the clone until its mapping descendants have drained.
async fn retained_mapping_source<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    program: Program<'db>,
) -> RunResult<OwnedMappingSource<'db, A>> {
    let quote = A::retained_clone_quote().and_then(|(work, bytes)| {
        // The access's construction/return carriers and two program carriers are internal
        // to this factory. The transfer helper quotes the completed wrapper separately.
        // Move the access (1), pass/install the program (2), and form/retire the wrapper (2).
        let bytes = size_of::<A>()
            .checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<Program<'db>>().checked_mul(2)?))
            .and_then(|n| n.checked_add(bytes))
            .ok_or(RunError::Contract("retained mapping source byte quotation overflow"))?;
        let work = work
            .checked_add(5)
            .ok_or(RunError::Contract("retained mapping source work quotation overflow"))?;
        Ok((work, bytes))
    });
    local_quoted_with_fixed_transfers_at(access.endpoint(), quote, || OwnedMappingSource {
        access: A::clone(access),
        program,
    })
    .await
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Maps one signature with a new retained visitor for its annotation descendants.
    pub(in crate::types::infer) async fn apply_signature_mapping(
        &self,
        signature: &Signature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
    ) -> RunResult<Signature<'db>> {
        let source = retained_mapping_source(self.access, program).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_signature_mapping(
                self.db(), signature, program, mapping, source,
            )
        }).await
    }

    /// Constructs and awaits a mapping dependency after admitting its future and fixed transfers.
    /// `local_quoted_with_fixed_transfers` retains the factory outside its `TaskEndpoint::local_call`
    /// callback. That boundary suspends refusal while the driver drains the current invocation's
    /// queued semantic children, keeping captured partial values alive until that drainage finishes.
    /// Successful construction transfers those captures into the new future.
    async fn function_mapping_child<T, F, M>(&self, make: M) -> RunResult<T>
    where
        F: Future<Output = RunResult<T>>,
        M: FnOnce() -> F,
    {
        let future = self.boxed_future_with_fixed_transfers(Ok((0, 0)), make).await?;
        future.await
    }

    pub(in crate::types::infer) async fn compose_source_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.allocate_future(|| compose_specialization_root_with(self.db(), base, additional, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CompositionStartEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Environment = Program<'db>;

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        SpecializationEffects::generic_context(self, db, specialization).await
    }
    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<Program<'db>> {
        SpecializationEffects::program(self, db, context).await
    }
    async fn environment(&self, program: Program<'db>) -> RunResult<Self::Environment> {
        SpecializationEffects::environment(self, program).await
    }
    async fn compose_fresh(
        &self,
        _db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        env: &Self::Environment,
    ) -> RunResult<Specialization<'db>> {
        let source = retained_mapping_source(self.access, *env).await?;
        self.function_mapping_child(|| {
            self.access.resources().compose_specialization(
                self.db(),
                base,
                additional,
                *env,
                source,
            )
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn materialize(
        &self,
        ty: Type<'db>,
        program: Program<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        self.apply_mapping(ty, program, OwnedTypeMapping::Materialize(kind))
            .await
    }

    pub(in crate::types::infer) async fn bind_legacy_typevars(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        binding: BindingContext<'db>,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        let mapping = self
            .local(1, size_of::<OwnedTypeMapping<'run, 'db>>() * 2, || {
                OwnedTypeMapping::BindLegacyTypevars(binding)
            })
            .await?;
        self.apply_mapping(ty, program, mapping).await
    }

    pub(in crate::types::infer) async fn apply_specialization_query(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> RunResult<Type<'db>> {
        shared_specialization_with(self.db(), ty, specialization, specialize_self_domain, self)
            .await
    }

    pub(in crate::types::infer) async fn apply_partial_specialization(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        generic_context: GenericContext<'db>,
        types: InitializedTypePrefix<'run, 'db>,
        skip: Option<usize>,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        let mapping = self
            .local(1, size_of::<OwnedTypeMapping<'run, 'db>>() * 2, || {
                OwnedTypeMapping::Partial {
                    generic_context,
                    types,
                    skip,
                }
            })
            .await?;
        self.apply_mapping(ty, program, mapping).await
    }

    /// Retains a completed insert-only variable map before starting its callable's mapper.
    /// Its construction, backing allocation, and failed-transfer cleanup are already admitted.
    pub(in crate::types::infer) async fn retain_return_typevars(
        &self,
        values: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<RetainedReturnTypevars<'run, 'db>> {
        self.function_mapping_child(|| {
            self.access.resources().retain_return_typevars(self.access.endpoint(), values)
        }).await
    }

    /// Retains completed insert-only callable replacements before mapping the original return type.
    /// Its construction, backing allocation, and failed-transfer cleanup are already admitted.
    pub(in crate::types::infer) async fn retain_return_callables(
        &self,
        values: FxHashMap<CallableType<'db>, CallableType<'db>>,
    ) -> RunResult<RetainedReturnCallables<'run, 'db>> {
        self.function_mapping_child(|| {
            self.access.resources().retain_return_callables(self.access.endpoint(), values)
        }).await
    }

    /// Maps one callable's signatures with a fresh retained visitor without interning a callable.
    /// The caller adds a generic context containing the renamed variables to the mapped
    /// signatures before interning the replacement callable.
    pub(in crate::types::infer) async fn apply_callable_signature_mapping(
        &self,
        signatures: &CallableSignature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
    ) -> RunResult<CallableSignature<'db>> {
        let source = retained_mapping_source(self.access, program).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_callable_signature_mapping(
                self.db(), signatures, program, mapping, source,
            )
        }).await
    }

    pub(in crate::types::infer) async fn apply_parameter_mapping(
        &self,
        parameters: &Parameters<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
    ) -> RunResult<Parameters<'db>> {
        let source = retained_mapping_source(self.access, program).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_parameter_mapping(
                self.db(), parameters, program, mapping, source,
            )
        }).await
    }

    pub(in crate::types::infer) async fn apply_mapping(
        &self,
        ty: Type<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
    ) -> RunResult<Type<'db>> {
        let source = retained_mapping_source(self.access, program).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_mapping(self.db(), ty, program, mapping, source)
        }).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassBaseSpecializationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Environment = Program<'db>;

    async fn checkpoint(&self, work: ClassBaseSpecializationWork) -> RunResult<()> {
        let quote = work.quote();
        self.local(quote.work, quote.bytes, || ()).await
    }

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        SpecializationEffects::generic_context(self, db, specialization).await
    }

    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<Program<'db>> {
        SpecializationEffects::program(self, db, context).await
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<Self::Environment> {
        SpecializationEffects::environment(self, program).await
    }

    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        SpecializationEffects::materialization_kind(self, db, specialization).await
    }

    async fn map_base_fresh(
        &self,
        _db: &'db dyn Db,
        base: ClassBase<'db>,
        env: &Self::Environment,
        mapping: ClassBaseMapping<'db>,
    ) -> RunResult<ClassBase<'db>> {
        let source = retained_mapping_source(self.access, *env).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_class_base_mapping(
                self.db(),
                base,
                *env,
                mapping,
                source,
            )
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SpecializationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Environment = Program<'db>;

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.field(specialization.field_requests(db).generic_context())
            .await
    }

    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<Program<'db>> {
        self.field(context.field_requests(db).program()).await
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<Self::Environment> {
        self.local(2, size_of::<Program<'db>>() * 2, || {
            self.check_program(program)?;
            Ok(program)
        })
        .await?
    }

    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.field(specialization.field_requests(db).materialization_kind())
            .await
    }

    async fn mapping(
        &self,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        materialization_kind: Option<MaterializationKind>,
    ) -> RunResult<OwnedTypeMapping<'db, 'db>> {
        self.local(1, size_of::<OwnedTypeMapping<'db, 'db>>() * 2, || {
            OwnedTypeMapping::Specialization {
                specialization,
                specialize_self_domain,
                materialization_kind,
            }
        })
        .await
    }

    async fn map_fresh(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
        program: Self::Environment,
        mapping: OwnedTypeMapping<'db, 'db>,
    ) -> RunResult<Type<'db>> {
        self.apply_mapping(ty, program, mapping).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MappingSourceEffects<'run, 'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        self.access.endpoint()
    }

    async fn promotion_scalar(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        SourceEffects::promotion_scalar(self, env, class).await
    }

    async fn promotion_nominal_known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        SourceEffects::promotion_nominal_known_class(self, instance).await
    }

    async fn promotion_explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.class_object_child(|| NominalKnownClassEffects::explicit_any_class(self, class)).await
    }

    async fn promotion_intern_explicit_any(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<ExplicitAnyInstanceClass<'db>> {
        self.class_object_child(|| self.access.intern_explicit_any_class(class)).await
    }

    async fn promotion_numeric_union(
        &self,
        env: &ProgramEnvironment<'db>,
        union: KnownUnion,
    ) -> RunResult<Type<'db>> {
        SourceEffects::promotion_numeric_union(self, env, union).await
    }

    async fn function_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.function_mapping_child(|| SourceEffects::function_signature(self, function))
            .await
    }

    async fn function_has_separate_implementation(
        &self,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<bool> {
        self.function_mapping_child(|| literal.has_separate_implementation_with(self.db(), self))
            .await
    }

    async fn function_implementation_callable(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<CallableType<'db>> {
        let signature = self
            .function_mapping_child(|| {
                SourceEffects::function_last_definition_signature(self, function)
            })
            .await?;
        // Cloning shares parameter/constraint Arcs but may copy an extras box. Prepay final
        // ownership of those shared entries as well as the single inline overload's retirement.
        let quote = self
            .local_with_fixed_transfers(32, 0, || {
                let work = signature.retirement_work()?.checked_add(8)?;
                let bytes = signature
                    .clone_requested_bytes()?
                    .checked_add(size_of::<Signature<'db>>())?;
                Some((work, bytes))
            })
            .await?;
        let quote = quote.ok_or(RunError::Contract(
            "implementation signature clone quotation overflow",
        ));
        let signatures = self
            .local_quoted_with_fixed_transfers(quote, || {
                CallableSignature::single(signature.clone())
            })
            .await?;
        MappingSourceEffects::owned_mapped_callable(
            self,
            signatures,
            CallableTypeKind::Regular,
            None,
        )
        .await
    }

    async fn intern_mapped_function(
        &self,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>> {
        self.function_mapping_child(|| {
            self.access.intern_function_type(literal, updated, descriptor_kind)
        })
        .await
    }

    async fn owned_mapped_callable(
        &self,
        signatures: CallableSignature<'db>,
        kind: CallableTypeKind,
        deprecated: Option<OverloadLiteral<'db>>,
    ) -> RunResult<CallableType<'db>> {
        let quote = self.local_with_fixed_transfers(
            8,
            size_of::<usize>() * 6 + size_of::<Option<usize>>() * 2,
            || {
                size_of::<(
                    CallableSignature<'db>,
                    CallableTypeKind,
                    Option<OverloadLiteral<'db>>,
                )>()
                .checked_mul(2)
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<CallableType<'db>>>() * 2))
                .map(|bytes| (8, bytes))
                .ok_or(RunError::Contract("mapped callable input quotation overflow"))
            },
        ).await?;
        self.local_quoted_with_fixed_transfers(quote, || ()).await?;
        self.function_mapping_child(|| self.access.owned_callable_type(signatures, kind, deprecated))
            .await
    }

    async fn finish_owned_parameters(
        &self,
        parameters: Vec<Parameter<'db>>,
        kind: ParametersKind<'db>,
    ) -> RunResult<Parameters<'db>> {
        self.function_mapping_child(|| SourceEffects::finish_owned_parameters(self, parameters, kind)).await
    }

    async fn new_mapping_parameters(&self, capacity: usize) -> RunResult<Vec<Parameter<'db>>> {
        self.function_mapping_child(|| SourceEffects::new_mapping_parameters(self, capacity)).await
    }

    async fn push_mapping_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> RunResult<()> {
        self.function_mapping_child(|| SourceEffects::push_mapping_parameter(self, parameters, parameter)).await
    }

    async fn new_mapping_overloads(&self, capacity: usize) -> RunResult<SmallVec<[Signature<'db>; 1]>> {
        self.function_mapping_child(|| SourceEffects::new_mapping_overloads(self, capacity)).await
    }

    async fn push_mapping_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> RunResult<()> {
        self.function_mapping_child(|| SourceEffects::push_mapping_overload(self, overloads, signature)).await
    }

    async fn mapping_box_local<T, U>(&self, action: impl FnOnce() -> U) -> RunResult<U> {
        self.function_mapping_child(|| SourceEffects::mapping_box_local::<T, U>(self, action)).await
    }

    async fn mapping_parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> RunResult<ParameterKind<'db>> {
        self.function_mapping_child(|| SourceEffects::mapping_parameter_kind_local(self, action)).await
    }

    async fn new_mapping_tuple_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>> {
        self.function_mapping_child(|| SourceEffects::new_mapping_tuple_elements(self, capacity)).await
    }

    async fn push_mapping_tuple_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.function_mapping_child(|| SourceEffects::push_mapping_tuple_element(self, elements, ty)).await
    }

    async fn finish_mapping_tuple_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>> {
        self.function_mapping_child(|| SourceEffects::finish_mapping_tuple_elements(self, elements, variable)).await
    }

    async fn new_return_context_variables(&self, capacity: usize) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        let quote = ordered_merge::<BoundTypeVarInstance<'db>>(0, 0, capacity)
            .ok_or(RunError::Contract("return context allocation quotation overflow"));
        self.local_quoted_with_fixed_transfers(quote.map(|quote| (quote.work, quote.bytes)), || {
            FxOrderSet::with_capacity_and_hasher(capacity, BuildHasherDefault::default())
        }).await
    }

    async fn insert_return_context_variable(&self, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> RunResult<()> {
        let quote = self.local_with_fixed_transfers(4, 0, || {
            ordered_merge::<BoundTypeVarInstance<'db>>(variables.len(), variables.capacity(), 1)
        }).await?.ok_or(RunError::Contract("return context insertion quotation overflow"));
        self.local_quoted_with_fixed_transfers(quote.map(|quote| (quote.work, quote.bytes)), || {
            variables.insert(variable);
        }).await
    }

    async fn finish_return_context(&self, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> RunResult<GenericContext<'db>> {
        self.function_mapping_child(|| self.context_from_legacy_variables(env, variables)).await
    }

    async fn specialize_context_declaration(&self, variable: BoundTypeVarInstance<'db>, specialization: Specialization<'db>, specialize_self_domain: bool) -> RunResult<Type<'db>> {
        let mapping = self.local_with_fixed_transfers(3, 0, || OwnedTypeMapping::Specialization {
            specialization, specialize_self_domain, materialization_kind: None,
        }).await?;
        self.function_mapping_child(|| self.apply_mapping(Type::TypeVar(variable), self.program, mapping)).await
    }

    async fn retained_self_environment(&self, binding: BindingContext<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let env = self.local_with_fixed_transfers(5, 2 * size_of::<ProgramEnvironment<'db>>(), || match binding {
            BindingContext::Definition(definition) => ProgramEnvironment::from_definition(definition),
            BindingContext::Synthetic(program) => ProgramEnvironment::from_program(program),
        }).await?;
        let program = self.environment_program(&env).await?;
        Ok(ProgramEnvironment::from_program(program))
    }

    async fn retained_self_bounds(&self, bounds: crate::types::TypeVarBoundOrConstraints<'db>, specialization: Specialization<'db>) -> RunResult<crate::types::TypeVarBoundOrConstraints<'db>> {
        let source = retained_mapping_source(self.access, self.program).await?;
        self.function_mapping_child(|| {
            self.access.resources().apply_retained_bounds(self.db(), bounds, self.program, specialization, source)
        }).await
    }

    async fn freshen_context_declaration(
        &self,
        variable: BoundTypeVarInstance<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
    ) -> RunResult<Type<'db>> {
        let mapping = self.local_with_fixed_transfers(2, 0, || {
            OwnedTypeMapping::FreshenBoundTypeVars { generic_context, delta }
        }).await?;
        self.function_mapping_child(|| self.apply_mapping(Type::TypeVar(variable), self.program, mapping)).await
    }

    async fn finish_freshening_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> RunResult<GenericContext<'db>> {
        self.function_mapping_child(|| self.constructor_context_from_variables(env, variables)).await
    }

    async fn freshening_bounds(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<crate::types::TypeVarBoundOrConstraints<'db>>> {
        self.function_mapping_child(|| typevar_bounds_with(variable, env, self)).await
    }

    async fn freshening_bound_default(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Type<'db>>> {
        self.function_mapping_child(|| self.access.bound_typevar_default(variable)).await
    }

    async fn intern_freshening_constraints(&self, elements: &mut Vec<Type<'db>>) -> RunResult<TypeVarConstraints<'db>> {
        let (len, capacity) = self.local_with_fixed_transfers(2, 0, || (elements.len(), elements.capacity())).await?;
        let bytes = std::alloc::Layout::array::<Type<'db>>(len)
            .ok().map(|layout| layout.size());
        let quote = len.checked_mul(3)
            .and_then(|work| work.checked_add(8))
            .zip(if len == capacity { Some(0) } else { bytes })
            .ok_or(RunError::Contract("freshening constraints boxing quotation overflow"));
        let elements = self.local_quoted_with_fixed_transfers(quote, || {
            std::mem::take(elements).into_boxed_slice()
        }).await?;
        self.function_mapping_child(|| self.access.intern_typevar_constraints(elements)).await
    }

    async fn intern_freshening_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<crate::types::TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.function_mapping_child(|| self.access.intern_typevar_instance(identity, bounds, variance, default)).await
    }

    async fn intern_freshening_bound(
        &self,
        variable: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.function_mapping_child(|| self.access.intern_bound_typevar(variable, identity)).await
    }

    async fn promotion_function_callable(&self, function: FunctionType<'db>) -> RunResult<Type<'db>> {
        let callable = self
            .function_mapping_child(|| function.into_callable_type_with(self.db(), self))
            .await?;
        self.local_with_fixed_transfers(1, 0, || Type::Callable(callable))
            .await
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        let quote = generated_field_quote(
            |alias: GenericAlias<'db>, context| alias.field_requests(context),
            |alias: GenericAlias<'db>, context| alias.field_requests(context).specialization(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = alias.field_requests(endpoint.field_request_context()).specialization();
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        let quote = generated_field_quote(
            |alias: GenericAlias<'db>, context| alias.field_requests(context),
            |alias: GenericAlias<'db>, context| alias.field_requests(context).origin(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = alias.field_requests(endpoint.field_request_context()).origin();
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }

    async fn intern_mapped_alias(
        &self,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericAlias<'db>> {
        self.function_mapping_child(|| self.access.intern_generic_alias(origin, specialization)).await
    }

    async fn unavailable<T>(
        &self,
        mapping: OwnedTypeMapping<'run, 'db>,
        operation: MaterializationOperation,
    ) -> RunResult<T> {
        let operation = match mapping {
            OwnedTypeMapping::Materialize(_) => SourceOperation::Materialization(operation),
            OwnedTypeMapping::BindSelf(_) => SourceOperation::SelfMapping(operation),
            OwnedTypeMapping::FreshenBoundTypeVars { .. } => SourceOperation::FreshenBoundTypeVars(operation),
            OwnedTypeMapping::ReturnCallables(_) | OwnedTypeMapping::RescopeReturnCallables(_) => SourceOperation::ReturnCallableMapping(operation),
            OwnedTypeMapping::PromoteRegular(_) => SourceOperation::PublicPromotion(operation),
            OwnedTypeMapping::Partial { .. } | OwnedTypeMapping::Single { .. } | OwnedTypeMapping::Specialization { .. } => SourceOperation::Specialization(operation),
            OwnedTypeMapping::BindLegacyTypevars(_) => {
                SourceOperation::LegacyTypeVarBinding(operation)
            }
        };
        SourceEffects::unavailable(self, operation).await
    }

    async fn should_bind_self(
        &self,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.should_bind_self_mapping(env, binding, variable).await
    }

    async fn bind_typevar(
        &self,
        variable: TypeVarInstance<'db>,
        binding: BindingContext<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.bind_typevar_in_context(variable, binding).await
    }

    async fn typeform_argument(&self, typeform: TypeFormType<'db>) -> RunResult<Type<'db>> {
        self.field(typeform.field_requests(self.db()).type_argument())
            .await
    }

    async fn intern_typeform(&self, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.access.intern_typeform(argument).await
    }

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        TupleSpecEffects::exact_spec(self, tuple).await
    }

    async fn construct_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        tuple_type(self.db(), env, spec, self).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn new_union(&self, env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>> {
        self.environment_program(env).await?;
        let builder = PairUnionEffects::new_union(self, env).await?;
        self.local(size_of::<UnionBuilder<'db>>() * 2 + 1, 0, || {
            builder.unpack_aliases(false)
        })
        .await
    }

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        PairUnionEffects::union_add(self, builder, ty).await
    }

    async fn finish_union(
        &self,
        mut builder: UnionBuilder<'db>,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<Type<'db>> {
        UnionEffects::merge_recursion(self, &mut builder, recursively_defined).await?;
        PairUnionEffects::union_build(self, builder).await
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::positive_elements(self, intersection).await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::negative_elements(self, intersection).await
    }

    async fn next_intersection(
        &self,
        elements: &mut Elements<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        InsertionEffects::next_element(self, elements).await
    }

    async fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        SourceEffects::new_intersection(self, env).await
    }

    async fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_positive(builder, ty).await
    }

    async fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_negative(builder, ty).await
    }

    async fn finish_intersection(
        &self,
        mut builder: IntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        let result = self.intersection_build(&mut builder).await?;
        self.retire_intersection(builder).await?;
        Ok(result)
    }
    async fn specialization_context(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        let quote = generated_field_quote(
            |specialization: Specialization<'db>, context| specialization.field_requests(context),
            |specialization: Specialization<'db>, context| specialization.field_requests(context).generic_context(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = specialization.field_requests(endpoint.field_request_context()).generic_context();
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }
    async fn specialization_types(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let quote = generated_field_quote(
            |specialization: Specialization<'db>, context| specialization.field_requests(context),
            |specialization: Specialization<'db>, context| specialization.field_requests(context).types(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = specialization.field_requests(endpoint.field_request_context()).types();
            endpoint.read_field(request, &TypeSliceDeref)
        }).await?;
        Ok(read.await)
    }
    async fn specialization_variables(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        let quote = generated_field_quote(
            |context: GenericContext<'db>, request_context| context.field_requests(request_context),
            |context: GenericContext<'db>, request_context| context.variables_request(request_context),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = context.variables_request(endpoint.field_request_context());
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }
    async fn specialization_materialization_kind(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        let quote = generated_field_quote(
            |specialization: Specialization<'db>, context| specialization.field_requests(context),
            |specialization: Specialization<'db>, context| specialization.field_requests(context).materialization_kind(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = specialization.field_requests(endpoint.field_request_context()).materialization_kind();
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }
    async fn specialization_tuple(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<TupleType<'db>>> {
        let quote = generated_field_quote(
            |specialization: Specialization<'db>, context| specialization.field_requests(context),
            |specialization: Specialization<'db>, context| specialization.tuple_request(context),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = specialization.tuple_request(endpoint.field_request_context());
            endpoint.read_field(request, &FixedMappingField)
        }).await?;
        Ok(read.await)
    }
    async fn intern_mapped_specialization(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>> {
        let (len, capacity, owned) = self
            .local_with_fixed_transfers(3, 0, || match &types {
                Cow::Borrowed(types) => (types.len(), types.len(), false),
                Cow::Owned(types) => (types.len(), types.capacity(), true),
            })
            .await?;
        let mut owner = Some(types);
        let quote = len.checked_mul(3).and_then(|work| work.checked_add(8))
            .zip(if owned && len == capacity { Some(0) } else {
                std::alloc::Layout::array::<Type<'db>>(len).ok().map(|layout| layout.size())
            })
            .ok_or(RunError::Contract("mapped specialization boxing quotation overflow"));
        let types = self.local_quoted_with_fixed_transfers(quote, || {
                match owner.take() {
                    Some(Cow::Borrowed(types)) => Ok(Box::<[Type<'db>]>::from(types)),
                    Some(Cow::Owned(types)) => Ok(types.into_boxed_slice()),
                    None => Err(RunError::Contract(
                        "mapped specialization payload was already transferred",
                    )),
                }
            }).await??;
        self.function_mapping_child(|| self.access.intern_specialization(context, types, kind, tuple)).await
    }
}
