//! Function and callable mapping borrow the existing mapping visitor and canonical source routes.

use std::future::Future;

use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use smallvec::SmallVec;

use super::{
    FixedMappingField, MappingSourceEffects, MaterializationOperation, OwnedTypeMapping, RetainedMappingSource,
    SourceMapping,
};
use crate::Db;
use crate::{FxOrderSet, Program, ProgramEnvironment};
use crate::types::constraints::control::hash_slots;
use crate::types::generics::context_construction::{ContextConstructionEffects, ContextVariables, context_from_typevars_with};
use crate::types::generics::mapping::SpecializationLookupEffects;
use super::typevar::SourceSpecialization;
use crate::types::generics::signature_context::{ContextSpecializationEffects, next_specialized_context_variable_with};
use crate::types::generics::return_callable_context::{ReturnCallableContextEffects, map_return_callable_context_with};
use crate::types::generics::signature_freshening::{SignatureFresheningEffects, freshen_signature_context_with};
use crate::types::mapping::return_callables::{ReturnCallableReplacements, ReturnTypevarReplacements};
use crate::types::typevar::{BoundTypeVarIdentity, BoundTypeVarInstance};
use crate::types::typevar::specialization::TypeVarSpecializationEffects;
use crate::types::ApplySpecialization;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::function::mapping::{FunctionMappingEffects, map_function_with, union_paramspec_candidates_with};
use crate::types::function::{FunctionLiteral, FunctionType, UpdatedFunctionSignatures};
use crate::types::local_transfer::{boxed_future_with_fixed_transfers_at, generated_field_quote, local_quoted_with_fixed_transfers_at};
use crate::types::mapping::effects::{MappingOperation, SharedMappingEffects};
use crate::types::signatures::mapping::{
    SignatureMappingEffects, map_callable_signature_with, map_callable_type_with,
};
use crate::types::signatures::{CallableSignature, Parameter, ParameterKind, Parameters, ParametersKind, Signature};
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, GenericContext, SignatureGenericContextMapping, Type, TypeContext, TypeMapping,
};

/// Borrows ordered declarations while the existing context constructor requests each mapped result.
#[derive(Debug)]
pub(super) struct SpecializedContextInput<'db> {
    variables: &'db ContextVariables<'db>,
    cursor: usize,
    specialization: SourceSpecialization<'db>,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    ContextSpecializationEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn next(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        // This step also prepays the shared keep/drop decision and optional result packaging.
        self.function_local(Some(12), Some(2 * size_of::<Option<Type<'db>>>() + 2 * size_of::<bool>()), || {
            let variable = GenericContext::variable_at_in(variables, *cursor);
            if variable.is_some() {
                *cursor += 1;
            }
            variable
        }).await
    }

    async fn lookup(&self, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<Type<'db>>> {
        let source = self.function_local(Some(5), Some(0), || {
            SourceSpecialization::from_specialization(specialization)
        }).await?;
        match source {
            Some(source) => self.function_child(|| self.source_specialization_lookup(source, variable)).await,
            None => self.unavailable(MaterializationOperation::Mode).await,
        }
    }

    async fn same_identity(&self, mapped: BoundTypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        let mapped = TypeVarSpecializationEffects::identity(self, mapped).await?;
        let original = TypeVarSpecializationEffects::identity(self, original).await?;
        self.function_local(Some(8), Some(0), || mapped == original).await
    }

    async fn map_retained(&self, _env: &ProgramEnvironment<'db>, specialization: &ApplySpecialization<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let stored = self.function_local(Some(3), Some(0), || match specialization {
            ApplySpecialization::Specialization { specialization, specialize_self_domain } => Some((*specialization, *specialize_self_domain)),
            _ => None,
        }).await?;
        let Some((specialization, specialize_self_domain)) = stored else {
            return self.unavailable(MaterializationOperation::Mode).await;
        };
        let mapped = self.function_child(|| async {
            self.source.effects().specialize_context_declaration(variable, specialization, specialize_self_domain).await
        }).await?;
        self.function_local(Some(2), Some(0), || mapped.as_typevar()).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    ContextConstructionEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    type Input = SpecializedContextInput<'db>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        ContextConstructionEffects::program(&self.source.effects(), env).await
    }

    async fn input_lower_bound(&self, _input: &Self::Input) -> RunResult<usize> {
        self.function_local(Some(1), Some(0), || 0).await
    }

    async fn next_variable(&self, input: &mut Self::Input) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let (specialization, specialize_self_domain) = self.function_local(Some(12), Some(size_of::<SourceSpecialization<'db>>() + 2 * size_of::<ApplySpecialization<'db, 'db>>()), || {
            let specialization = input.specialization.into_specialization();
            (specialization, specialization.specialize_self_domain())
        }).await?;
        self.function_child(|| next_specialized_context_variable_with(
            self.visitor.env, input.variables, &mut input.cursor, &specialization, specialize_self_domain, self,
        )).await
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        ContextConstructionEffects::new_variables(&self.source.effects(), lower_bound).await
    }

    async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<BoundTypeVarIdentity<'db>> {
        TypeVarSpecializationEffects::identity(self, variable).await
    }

    async fn insert(&self, variables: &mut ContextVariables<'db>, identity: BoundTypeVarIdentity<'db>, variable: BoundTypeVarInstance<'db>) -> RunResult<()> {
        ContextConstructionEffects::insert(&self.source.effects(), variables, identity, variable).await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        ContextConstructionEffects::shrink(&self.source.effects(), variables).await
    }

    async fn intern(&self, program: Program<'db>, variables: ContextVariables<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::intern(&self.source.effects(), program, variables).await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::publish(&self.source.effects(), context).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    /// Runs one finite mapping step after admitting payload work and fixed callback/result transfers.
    /// The current invocation can have queued semantic children. `TaskEndpoint::local_call` suspends
    /// refusal until the driver drains them; keeping the action outside that callback retains its
    /// partial values with the invocation throughout drainage instead of dropping them on refusal.
    pub(super) async fn function_local<T, F: FnOnce() -> T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: F,
    ) -> RunResult<T> {
        let quote = work.zip(requested_bytes).ok_or(RunError::Contract(
            "function mapping local quotation overflow",
        ));
        local_quoted_with_fixed_transfers_at(self.endpoint, quote, action).await
    }

    /// Admits, constructs and awaits a shared mapper or source-operation future, returning its result.
    /// The local action retains the factory's captures through refused construction and child drainage.
    pub(super) async fn function_child<T, F, M>(&self, make: M) -> RunResult<T>
    where
        F: Future<Output = RunResult<T>>,
        M: FnOnce() -> F,
    {
        let future = boxed_future_with_fixed_transfers_at(self.endpoint, Ok((0, 0)), make).await?;
        future.await
    }

    /// Returns `None` to continue normal mapping when no overload has a union-valued substitution.
    /// A discovered union requires unsupported expansion and refuses `FunctionParamSpecPrelude`.
    pub(super) async fn check_union_paramspec_expansion(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let specialization = self.function_local(Some(3), Some(0), || {
            self.check_visitor(visitor)?;
            Ok::<_, RunError>(match mapping {
                TypeMapping::ApplySpecialization(specialization)
                | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => Some(specialization),
                _ => None,
            })
        }).await??;
        let Some(specialization) = specialization else {
            return Ok(None);
        };
        let mut candidates = self.function_child(|| {
            union_paramspec_candidates_with(db, ty, specialization, self)
        }).await?;
        let candidate = self.function_child(|| candidates.next_with(db, specialization, self)).await?;
        match candidate {
            Some(_) => self.unavailable(MaterializationOperation::Leaf(MappingOperation::FunctionParamSpecPrelude)).await,
            None => Ok(None),
        }
    }

    /// Maps a function's stored and effective signatures, returning a function literal.
    pub(super) async fn map_source_function(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.check_function_mapping(visitor).await?;
        let mapped = self
            .function_child(|| map_function_with(db, function, mapping, tcx, visitor, self))
            .await?;
        self.function_local(Some(1), Some(0), || Type::FunctionLiteral(mapped))
            .await
    }

    /// Maps a callable's signatures while retaining its kind and deprecation metadata.
    /// `RescopeReturnCallables` instead replaces the handle without reading those fields.
    pub(super) async fn map_source_callable(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.check_callable_mapping(visitor).await?;
        let mapped = self
            .function_child(|| map_callable_type_with(db, callable, mapping, tcx, visitor, self))
            .await?;
        self.function_local(Some(1), Some(0), || Type::Callable(mapped))
            .await
    }

    /// Admits default binding and retained partial substitution for callable values only.
    async fn check_callable_mapping(
        &self,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<()> {
        let default_mapping = self.function_local(Some(8), Some(0), || {
            self.check_visitor(visitor)?;
            Ok::<_, RunError>(matches!(
                self.mapping,
                OwnedTypeMapping::BindLegacyTypevars(_) | OwnedTypeMapping::Partial { .. }
            ))
        }).await??;
        if default_mapping {
            Ok(())
        } else {
            self.check_function_mapping(visitor).await
        }
    }

    /// Rejects unsupported source modes before entering shared function/signature mapping.
    async fn check_function_mapping(
        &self,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<()> {
        let supported = self
            .function_local(Some(2), Some(0), || {
                self.check_visitor(visitor)?;
                Ok::<_, RunError>(match self.mapping {
                    OwnedTypeMapping::PromoteRegular(_) | OwnedTypeMapping::BindSelf(_) | OwnedTypeMapping::ReturnCallables(_) | OwnedTypeMapping::RescopeReturnCallables(_) | OwnedTypeMapping::FreshenBoundTypeVars { .. } | OwnedTypeMapping::Specialization { materialization_kind: None, .. } | OwnedTypeMapping::Single { .. } => true,
                    OwnedTypeMapping::Partial { .. }
                    | OwnedTypeMapping::Materialize(_)
                    | OwnedTypeMapping::BindLegacyTypevars(_)
                    | OwnedTypeMapping::Specialization { materialization_kind: Some(_), .. } => false,
                })
            })
            .await??;
        if supported {
            Ok(())
        } else {
            self.unavailable(MaterializationOperation::Mode).await
        }
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SignatureMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.function_local(work, requested_bytes, action).await
    }

    async fn finish_parameters(
        &self,
        parameters: Vec<Parameter<'db>>,
        kind: ParametersKind<'db>,
    ) -> RunResult<Parameters<'db>> {
        self.function_child(|| async {
            self.source.effects().finish_owned_parameters(parameters, kind).await
        }).await
    }

    async fn new_parameters(&self, capacity: usize) -> RunResult<Vec<Parameter<'db>>> {
        self.function_child(|| async { self.source.effects().new_mapping_parameters(capacity).await }).await
    }

    async fn push_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> RunResult<()> {
        self.function_child(|| async { self.source.effects().push_mapping_parameter(parameters, parameter).await }).await
    }

    async fn new_overloads(&self, capacity: usize) -> RunResult<SmallVec<[Signature<'db>; 1]>> {
        self.function_child(|| async { self.source.effects().new_mapping_overloads(capacity).await }).await
    }

    async fn push_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> RunResult<()> {
        self.function_child(|| async { self.source.effects().push_mapping_overload(overloads, signature).await }).await
    }

    async fn box_local<T, U>(&self, action: impl FnOnce() -> U) -> RunResult<U> {
        self.function_child(|| async { self.source.effects().mapping_box_local::<T, U>(action).await }).await
    }

    async fn parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> RunResult<ParameterKind<'db>> {
        self.function_child(|| async { self.source.effects().mapping_parameter_kind_local(action).await }).await
    }

    async fn check_paramspec_specialization(
        &self,
        signature: &Signature<'db>,
        mapping: &TypeMapping<'_, 'db>,
    ) -> RunResult<()> {
        let (partial_tail, candidate) = self.function_local(Some(20), Some(size_of::<Option<(&[crate::types::signatures::Parameter<'db>], BoundTypeVarInstance<'db>)>>() * 2), || {
            match mapping {
                TypeMapping::ApplySpecialization(ApplySpecialization::Partial { .. }) => {
                    (signature.parameters().as_paramspec_with_prefix().is_some(), None)
                }
                TypeMapping::ApplySpecialization(specialization @ (ApplySpecialization::ReturnCallables(_) | ApplySpecialization::Specialization { .. } | ApplySpecialization::Single(..))) => {
                    (false, signature.parameters().as_paramspec_with_prefix().map(|(_, variable)| (specialization, variable)))
                }
                _ => (false, None),
            }
        }).await?;
        if partial_tail {
            return self.unavailable(MaterializationOperation::Leaf(MappingOperation::ParamSpec)).await;
        }
        if let Some((specialization, variable)) = candidate
            && self.function_child(|| TypeVarSpecializationEffects::other_lookup(self, specialization, variable)).await?.is_some()
        {
            return self.unavailable(MaterializationOperation::Leaf(MappingOperation::ParamSpec)).await;
        }
        self.function_local(Some(1), Some(0), || ()).await
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

    async fn update_generic_context(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<GenericContext<'db>> {
        let decision = self.function_local(Some(2), Some(0), || {
            self.check_visitor(visitor)?;
            Ok::<_, RunError>(mapping.signature_generic_context_mapping())
        }).await??;
        match decision {
            SignatureGenericContextMapping::Preserve => {
                self.function_local(Some(1), Some(0), || context).await
            }
            SignatureGenericContextMapping::Specialize(ApplySpecialization::ReturnCallables(replacements)) => {
                self.function_child(|| map_return_callable_context_with(db, visitor.env, context, replacements, self)).await
            }
            SignatureGenericContextMapping::RemoveSelf(_) => {
                self.unavailable(MaterializationOperation::Leaf(MappingOperation::GenericContextSelfRemoval)).await
            }
            SignatureGenericContextMapping::Freshen { generic_context, delta } => {
                self.function_child(|| freshen_signature_context_with(db, visitor.env, context, generic_context, delta, self)).await
            }
            SignatureGenericContextMapping::Specialize(specialization @ (ApplySpecialization::Specialization { .. } | ApplySpecialization::Single(..))) => {
                let variables = SpecializationLookupEffects::variables(self, context).await?;
                let input = self.function_local(Some(16), Some(2 * size_of::<SourceSpecialization<'db>>() + size_of::<Option<SourceSpecialization<'db>>>() + size_of::<Option<SpecializedContextInput<'db>>>() + size_of::<RunError>()), || {
                    SourceSpecialization::from_specialization(&specialization).map(|specialization| SpecializedContextInput {
                        variables, cursor: 0, specialization,
                    }).ok_or(RunError::Contract("signature context substitution changed after dispatch"))
                }).await??;
                self.function_child(|| context_from_typevars_with(db, visitor.env, input, self)).await
            }
            SignatureGenericContextMapping::Specialize(_)
            | SignatureGenericContextMapping::ReplaceSelf(_) => {
                self.unavailable(MaterializationOperation::Mode).await
            }
        }
    }

    async fn map_receiver_constraints(
        &self,
        _db: &'db dyn Db,
        signature: &Signature<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Option<OwnedConstraintSet<'db>>> {
        let terminal = self
            .function_local(Some(12), Some(size_of::<OwnedConstraintSet<'db>>() * 2), || {
                signature.map_terminal_receiver_constraints()
            })
            .await?;
        match terminal {
            Some(mapped) => self.function_local(Some(2), Some(0), || mapped).await,
            None => self.unavailable(MaterializationOperation::Leaf(
                MappingOperation::SignatureReceiverConstraints,
            ))
            .await,
        }
    }

    async fn expand_starred_parameters(
        &self,
        _db: &'db dyn Db,
        _parameters: &mut Parameters<'db>,
    ) -> RunResult<()> {
        self.unavailable(MaterializationOperation::Leaf(
            MappingOperation::SignatureStarredExpansion,
        ))
        .await
    }

    async fn return_callable_replacement(
        &self,
        replacements: ReturnCallableReplacements<'_, 'db>,
        callable: CallableType<'db>,
    ) -> RunResult<CallableType<'db>> {
        let (len, capacity) = self.function_local(Some(2), Some(0), || {
            (replacements.len(), replacements.capacity())
        }).await?;
        let slots = hash_slots::<RunError>(capacity).ok();
        let work = slots.and_then(|slots| slots.checked_mul(4))
            .and_then(|work| work.checked_add(len.checked_mul(4)?))
            .and_then(|work| work.checked_add(16));
        self.function_local(work, Some(size_of::<Option<&CallableType<'db>>>() * 2 + size_of::<u64>() * 8), || {
            replacements.get(callable).unwrap_or(callable)
        }).await
    }

    async fn callable_signatures(
        &self,
        _db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        let read = boxed_future_with_fixed_transfers_at(
            self.endpoint,
            generated_field_quote(
                |callable: CallableType<'db>, context| callable.field_requests(context),
                |callable: CallableType<'db>, context| callable.field_requests(context).signatures(),
            ),
            || self.endpoint.read_field(
                callable.field_requests(self.endpoint.field_request_context()).signatures(),
                &FixedMappingField,
            ),
        ).await?;
        Ok(read.await)
    }

    async fn replace_callable_signatures(
        &self,
        _db: &'db dyn Db,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> RunResult<CallableType<'db>> {
        let kind_read = boxed_future_with_fixed_transfers_at(
            self.endpoint,
            generated_field_quote(
                |callable: CallableType<'db>, context| callable.field_requests(context),
                |callable: CallableType<'db>, context| callable.field_requests(context).kind(),
            ),
            || self.endpoint.read_field(
                callable.field_requests(self.endpoint.field_request_context()).kind(),
                &FixedMappingField,
            ),
        ).await?;
        let kind = kind_read.await;
        let deprecated_read = boxed_future_with_fixed_transfers_at(
            self.endpoint,
            generated_field_quote(
                |callable: CallableType<'db>, context| callable.field_requests(context),
                |callable: CallableType<'db>, context| callable.field_requests(context).deprecated(),
            ),
            || self.endpoint.read_field(
                callable.field_requests(self.endpoint.field_request_context()).deprecated(),
                &FixedMappingField,
            ),
        ).await?;
        let deprecated = deprecated_read.await;
        self.function_child(|| async {
            self.source
                .effects()
                .owned_mapped_callable(signatures, kind, deprecated)
                .await
        })
        .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SignatureFresheningEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn local<T>(&self, work: Option<usize>, bytes: Option<usize>, action: impl FnOnce() -> T) -> RunResult<T> {
        self.function_local(work, bytes, action).await
    }

    async fn variables(&self, db: &'db dyn Db, context: GenericContext<'db>) -> RunResult<&'db ContextVariables<'db>> {
        ReturnCallableContextEffects::variables(self, db, context).await
    }

    async fn map_declaration(&self, _db: &'db dyn Db, _env: &ProgramEnvironment<'db>, variable: BoundTypeVarInstance<'db>, context: GenericContext<'db>, delta: u32) -> RunResult<Type<'db>> {
        self.function_child(|| async {
            self.source.effects().freshen_context_declaration(variable, context, delta).await
        }).await
    }

    async fn finish(&self, _db: &'db dyn Db, env: &ProgramEnvironment<'db>, variables: &[BoundTypeVarInstance<'db>]) -> RunResult<GenericContext<'db>> {
        self.function_child(|| async { self.source.effects().finish_freshening_context(env, variables).await }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    FunctionMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.function_local(work, bytes, action).await
    }

    async fn field<Q: FieldRequest<'db>>(&self, request: Q) -> RunResult<Q::Output> {
        self.function_child(|| async { Ok(self.endpoint.read_field(request, &FixedMappingField).await) })
            .await
    }

    async fn signature(
        &self,
        _db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.function_child(|| async { self.source.effects().function_signature(function).await })
            .await
    }

    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.function_child(|| SignatureMappingEffects::callable_signatures(self, db, callable)).await
    }

    async fn paramspec_substitution(
        &self,
        _db: &'db dyn Db,
        specialization: &ApplySpecialization<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        if self.function_local(Some(1), Some(0), || {
            matches!(specialization, ApplySpecialization::Partial { .. })
        }).await? {
            return self.unavailable(MaterializationOperation::Leaf(MappingOperation::ParamSpec)).await;
        }
        self.function_child(|| TypeVarSpecializationEffects::other_lookup(self, specialization, variable)).await
    }

    async fn has_separate_implementation(
        &self,
        _db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<bool> {
        self.function_child(|| async {
            self.source
                .effects()
                .function_has_separate_implementation(literal)
                .await
        })
        .await
    }

    async fn implementation_callable(
        &self,
        _db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> RunResult<CallableType<'db>> {
        self.function_child(|| async {
            self.source
                .effects()
                .function_implementation_callable(function)
                .await
        })
        .await
    }

    async fn map_signature(
        &self,
        db: &'db dyn Db,
        signature: &CallableSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<CallableSignature<'db>> {
        self.function_child(|| {
            map_callable_signature_with(db, signature, mapping, tcx, visitor, self)
        })
        .await
    }

    async fn map_callable(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<CallableType<'db>> {
        self.function_child(|| map_callable_type_with(db, callable, mapping, tcx, visitor, self))
            .await
    }

    async fn intern_function(
        &self,
        _db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>> {
        self.function_child(|| async {
            self.source
                .effects()
                .intern_mapped_function(literal, updated, descriptor_kind)
                .await
        })
        .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    ReturnCallableContextEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn local<T>(&self, work: Option<usize>, bytes: Option<usize>, action: impl FnOnce() -> T) -> RunResult<T> {
        self.function_local(work, bytes, action).await
    }

    async fn variables(&self, _db: &'db dyn Db, context: GenericContext<'db>) -> RunResult<&'db ContextVariables<'db>> {
        let request_context = self.function_local(Some(4), Some(0), || self.endpoint.field_request_context()).await?;
        let request = self.function_local(Some(4), Some(0), || context.variables_request(request_context)).await?;
        self.function_child(|| async { Ok(self.endpoint.read_field(request, &FixedMappingField).await) }).await
    }

    async fn lookup(&self, replacements: ReturnTypevarReplacements<'_, 'db>, variable: BoundTypeVarInstance<'db>) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.return_typevar_lookup(replacements, variable).await
    }

    async fn identity(&self, _db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) -> RunResult<BoundTypeVarIdentity<'db>> {
        TypeVarSpecializationEffects::identity(self, variable).await
    }

    async fn new_variables(&self, capacity: usize) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        self.function_child(|| async { self.source.effects().new_return_context_variables(capacity).await }).await
    }

    async fn insert(&self, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, variable: BoundTypeVarInstance<'db>) -> RunResult<()> {
        self.function_child(|| async { self.source.effects().insert_return_context_variable(variables, variable).await }).await
    }

    async fn finish(&self, _db: &'db dyn Db, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> RunResult<GenericContext<'db>> {
        self.function_child(|| async { self.source.effects().finish_return_context(env, variables).await }).await
    }
}
