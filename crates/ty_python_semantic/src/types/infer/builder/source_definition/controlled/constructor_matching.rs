//! Connects shared constructor freshening to canonical fields and retained mapping roots.

use std::future::Future;
use std::marker::PhantomData;
use std::slice;

use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::FixedFieldBorrow;
use super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::constructor_matching::ConstructorMatchingEffects;
#[cfg(test)]
use crate::types::call::bind::constructor_matching::{
    ConstructorMatchingOperation, ConstructorMatchingPhase,
};
use crate::types::call::bind::constructor_preparation::constructor_return_with;
use crate::types::class_selection::class_specialization_with;
use crate::types::generics::context_construction::{
    ContextConstructionEffects, ContextVariables, context_from_typevars_with,
};
use crate::types::mapping::OwnedTypeMapping;
use crate::types::signatures::Signature;
use crate::types::typevar::constructor_nonce::ConstructorNonceEffects;
#[cfg(test)]
use crate::types::typevar::constructor_nonce::ConstructorNonceOperation;
use crate::types::{
    BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext, Specialization,
    Type, TypeVarKind,
};
use crate::{Db, Program, ProgramEnvironment};

#[cfg(test)]
use crate::types::call::bind::constructor_matching::observations;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Awaits a matching child after admitting its fixed future, factory and result transfers.
    /// Borrowed binding trees and mapping inputs stay in the caller until the child drains.
    pub(in crate::types::infer) async fn constructor_matching_child<T, F>(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<T>
    where
        F: Future<Output = RunResult<T>>,
    {
        self.type_parameter_future(make).await?.await
    }

    /// Rebuilds an ordered generic context from borrowed variables, using canonical deduplication.
    pub(in crate::types::infer) async fn constructor_context_from_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> RunResult<GenericContext<'db>> {
        let input = self
            .local_with_fixed_transfers(2, 0, || variables.iter())
            .await?;
        let effects = self
            .local_with_fixed_transfers(2, 0, || ConstructorContextEffects {
                source: self,
                input: PhantomData,
            })
            .await?;
        self.constructor_matching_child(|| {
            context_from_typevars_with(self.db(), env, input, &effects)
        })
        .await
    }

    /// Reads a variable's scalar identity and kind without evaluating its bounds or default.
    async fn constructor_variable_kind(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarKind> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                variable.identity_request(self.access.endpoint().field_request_context())
            })
            .await?;
        let identity = self.field_with_profile(request, &FixedFieldCopy).await?;
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                identity
                    .identity
                    .field_requests(self.access.endpoint().field_request_context())
                    .kind()
            })
            .await?;
        self.field_with_profile(request, &FixedFieldCopy).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorNonceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_quoted_with_fixed_transfers(
            work.zip(requested_bytes).ok_or(RunError::Contract(
                "constructor matching quotation overflow",
            )),
            action,
        )
        .await
    }

    async fn variables(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                context.variables_request(self.access.endpoint().field_request_context())
            })
            .await?;
        self.field_with_profile(request, &FixedFieldBorrow).await
    }

    async fn binding_context(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BindingContext<'db>> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                variable.identity_request(self.access.endpoint().field_request_context())
            })
            .await?;
        let identity = self.field_with_profile(request, &FixedFieldCopy).await?;
        self.local_with_fixed_transfers(1, 0, || identity.binding_context)
            .await
    }

    #[cfg(test)]
    fn before_nonce(&self, operation: ConstructorNonceOperation) {
        observations::observe_before_nonce(operation);
    }

    #[cfg(test)]
    fn after_nonce(&self, operation: ConstructorNonceOperation) {
        observations::observe_after_nonce(operation);
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorMatchingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn class_specialization(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        self.constructor_matching_child(|| self.environment_program(env))
            .await?;
        let selected = self
            .constructor_matching_child(|| class_specialization_with(ty, self))
            .await?;
        self.local_with_fixed_transfers(2, 0, || selected.map(|(_, specialization)| specialization))
            .await
    }

    async fn specialization_context(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        let request = self
            .local_with_fixed_transfers(4, 0, || {
                specialization
                    .field_requests(self.access.endpoint().field_request_context())
                    .generic_context()
            })
            .await?;
        self.field_with_profile(request, &FixedFieldCopy).await
    }

    async fn is_paramspec(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let kind = self
            .constructor_matching_child(|| self.constructor_variable_kind(variable))
            .await?;
        self.local_with_fixed_transfers(1, 0, || kind.is_paramspec())
            .await
    }

    async fn is_self(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let kind = self
            .constructor_matching_child(|| self.constructor_variable_kind(variable))
            .await?;
        self.local_with_fixed_transfers(1, 0, || kind == TypeVarKind::TypingSelf)
            .await
    }

    async fn context_from_typevars(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> RunResult<GenericContext<'db>> {
        self.constructor_matching_child(|| self.constructor_context_from_variables(env, variables))
            .await
    }

    async fn freshen_type(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
    ) -> RunResult<Type<'db>> {
        let program = self
            .constructor_matching_child(|| self.environment_program(env))
            .await?;
        let mapping = self
            .local_with_fixed_transfers(2, 0, || OwnedTypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            })
            .await?;
        self.constructor_matching_child(|| self.apply_mapping(ty, program, mapping))
            .await
    }

    async fn freshen_signature(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
    ) -> RunResult<Signature<'db>> {
        let program = self
            .constructor_matching_child(|| self.environment_program(env))
            .await?;
        let mapping = self
            .local_with_fixed_transfers(2, 0, || OwnedTypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            })
            .await?;
        self.constructor_matching_child(|| {
            self.apply_signature_mapping(signature, program, mapping)
        })
        .await
    }

    async fn normalized_return(
        &self,
        _db: &'db dyn Db,
        signature: &Signature<'db>,
        kind: ConstructorCallableKind,
        instance: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.constructor_matching_child(|| constructor_return_with(signature, kind, instance, self))
            .await
    }

    async fn signature_retirement(&self, signature: &Signature<'db>) -> RunResult<usize> {
        let retirement = self
            .local_with_fixed_transfers(64, 0, || signature.retirement_work())
            .await?;
        Self::checked(retirement)
    }

    #[cfg(test)]
    fn before_matching(&self, operation: ConstructorMatchingOperation) {
        observations::observe_before_matching(operation);
    }

    #[cfg(test)]
    fn after_matching(&self, operation: ConstructorMatchingOperation) {
        observations::observe_after_matching(operation);
    }

    #[cfg(test)]
    fn matching_entry(&self, phase: ConstructorMatchingPhase, identity: usize) {
        observations::observe_entry(phase, identity);
    }
}

/// Supplies borrowed declaration order to the shared generic-context constructor.
struct ConstructorContextEffects<'source, 'data, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    input: PhantomData<&'data ()>,
}

impl<'data, 'run, 'db: 'run + 'data, A: SourceAccess<'run, 'db>> ContextConstructionEffects<'db>
    for ConstructorContextEffects<'_, 'data, '_, 'run, 'db, A>
{
    type Error = RunError;
    type Input = slice::Iter<'data, BoundTypeVarInstance<'db>>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        self.source
            .constructor_matching_child(|| self.source.environment_program(env))
            .await
    }

    async fn input_lower_bound(&self, input: &Self::Input) -> RunResult<usize> {
        self.source
            .local_with_fixed_transfers(1, 0, || input.len())
            .await
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.source
            .local_with_fixed_transfers(2, 0, || input.next().copied())
            .await
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        self.source
            .constructor_matching_child(|| {
                ContextConstructionEffects::new_variables(self.source, lower_bound)
            })
            .await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        let request = self
            .source
            .local_with_fixed_transfers(4, 0, || {
                variable.identity_request(self.source.access.endpoint().field_request_context())
            })
            .await?;
        self.source
            .field_with_profile(request, &FixedFieldCopy)
            .await
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.source
            .constructor_matching_child(|| {
                ContextConstructionEffects::insert(self.source, variables, identity, variable)
            })
            .await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        self.source
            .constructor_matching_child(|| {
                ContextConstructionEffects::shrink(self.source, variables)
            })
            .await
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.source
            .constructor_matching_child(|| {
                ContextConstructionEffects::intern(self.source, program, variables)
            })
            .await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        self.source
            .local_with_fixed_transfers(1, 0, || context)
            .await
    }
}
