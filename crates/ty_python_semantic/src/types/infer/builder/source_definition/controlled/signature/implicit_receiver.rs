//! Admits receiver annotation copies and borrows existing variables for canonical context rebuilding.

use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::{RunError, RunResult};

use super::super::{SourceAccess, SourceEffects};
use crate::types::generics::binding::{BindingFacts, TypeVarBindingEffects, find_in_context_with};
use crate::types::generics::context_construction::{
    ContextConstructionEffects, ContextVariables, context_from_typevars_with,
};
use crate::types::signatures::implicit_receiver::{
    ExplicitSelfEffects, ImplicitReceiverEffects, InstalledReceiver, install_annotation,
    receiver_is_eligible,
};
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::signatures::{Parameter, Signature};
use crate::types::{BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext, Type, TypeVarKind};
use crate::{Program, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExplicitSelfEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn variables(&self, context: GenericContext<'db>) -> RunResult<&'db ContextVariables<'db>> {
        TypeVarBindingEffects::variables(self, context).await
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        TypeVarBindingEffects::next_variable(self, variables, cursor).await
    }

    async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        let typevar = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        TypeVarBindingEffects::kind(self, typevar).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitReceiverEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Receiver = Type<'db>;

    async fn eligible(&self, signature: &Signature<'db>) -> RunResult<bool> {
        self.local_with_fixed_transfers(
            12,
            size_of::<Option<&Parameter<'db>>>() + 4 * size_of::<bool>(),
            || receiver_is_eligible(signature),
        )
        .await
    }

    async fn receiver(&self, input: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(2, 0, || Some(input)).await
    }

    async fn install_annotation(
        &self,
        signature: &mut Signature<'db>,
        receiver: Type<'db>,
    ) -> RunResult<Option<InstalledReceiver<'db>>> {
        let quote = self
            .local_with_fixed_transfers(
                24,
                16 * size_of::<usize>() + 16 * size_of::<Option<usize>>(),
                || receiver_copy_quote(signature.parameters().len()),
            )
            .await?;
        // Arc::make_mut can copy every parameter. Name clones retain their CharStr backing;
        // the copy quote includes these fixed clones and retirement of the new parameter array.
        self.local_quoted_with_fixed_transfers(quote, || install_annotation(signature, receiver))
            .await
    }

    async fn binds_receiver(
        &self,
        context: GenericContext<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let typevar = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        let found = self
            .type_parameter_future(|| find_in_context_with(context, typevar, BindingFacts, self))
            .await?
            .await?;
        self.local_with_fixed_transfers(1, 0, || found.is_some()).await
    }

    async fn variables(&self, context: GenericContext<'db>) -> RunResult<&'db ContextVariables<'db>> {
        TypeVarBindingEffects::variables(self, context).await
    }

    async fn prepend_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
        variables: Option<&'db ContextVariables<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        let make = || {
            let input = ReceiverContextInput { first: Some(variable), variables, cursor: 0 };
            let effects = ReceiverContextEffects(self);
            async move { context_from_typevars_with(self.db(), env, input, &effects).await }
        };
        let quote = context_future_quote(&make);
        let future = self.local_quoted_with_fixed_transfers(quote, || Box::pin(make())).await?;
        future.await
    }

    async fn set_context(&self, signature: &mut Signature<'db>, context: GenericContext<'db>) -> RunResult<()> {
        self.local_with_fixed_transfers(2, size_of::<Option<GenericContext<'db>>>(), || {
            signature.generic_context = Some(context);
        })
        .await
    }
}

/// Quotes a possible copy of all parameters, its fixed field clones, and eventual retirement.
fn receiver_copy_quote(count: usize) -> RunResult<(usize, usize)> {
    let quote = (|| {
        let storage = parameters_storage_quote(count)?;
        let work = storage.work.checked_add(count.checked_mul(12)?)?.checked_add(32)?;
        let bytes = storage.bytes
            .checked_add(count.checked_mul(size_of::<Parameter<'_>>())?.checked_mul(2)?)?
            .checked_add(2 * size_of::<Type<'_>>())?
            .checked_add(size_of::<Option<&mut Parameter<'_>>>())?
            .checked_add(size_of::<Option<BoundTypeVarInstance<'_>>>())?
            .checked_add(size_of::<InstalledReceiver<'_>>())?;
        Some((work, bytes))
    })();
    quote.ok_or(RunError::Contract("receiver parameter storage quotation overflow"))
}

/// Quotes context input setup and the concrete child future without running its factory.
fn context_future_quote<F: Future, M: FnOnce() -> F>(_: &M) -> RunResult<(usize, usize)> {
    size_of::<F>().checked_mul(2)
        .and_then(|bytes| bytes.checked_add(size_of::<M>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<F::Output>().checked_mul(4)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Pin<Box<F>>>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<ReceiverContextInput<'_>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<usize>().checked_mul(12)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Option<usize>>().checked_mul(12)?))
        .map(|bytes| (28, bytes))
        .ok_or(RunError::Contract("receiver context continuation quotation overflow"))
}

/// Yields the receiver once, then borrows the previous variables in their stored order.
#[derive(Debug)]
struct ReceiverContextInput<'db> {
    first: Option<BoundTypeVarInstance<'db>>,
    variables: Option<&'db ContextVariables<'db>>,
    cursor: usize,
}

/// Reuses canonical context storage with a receiver-first, borrowed input.
struct ReceiverContextEffects<'a, 'source, 'run, 'db: 'run, A>(
    &'a SourceEffects<'source, 'run, 'db, A>,
);

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ContextConstructionEffects<'db>
    for ReceiverContextEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;
    type Input = ReceiverContextInput<'db>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        ContextConstructionEffects::program(self.0, env).await
    }

    async fn input_lower_bound(&self, input: &Self::Input) -> RunResult<usize> {
        self.0.local_with_fixed_transfers(8, 3 * size_of::<usize>() + size_of::<Option<usize>>(), || {
            let previous = input.variables.map(|variables| variables.len()).unwrap_or(0);
            previous.checked_add(1).ok_or(RunError::Contract("receiver context length overflow"))
        }).await?
    }

    async fn next_variable(&self, input: &mut Self::Input) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let bytes = size_of::<Option<(&BoundTypeVarIdentity<'db>, &BoundTypeVarInstance<'db>)>>()
            + size_of::<Option<BoundTypeVarInstance<'db>>>() + size_of::<usize>() + size_of::<bool>();
        self.0.local_with_fixed_transfers(14, bytes, || {
            if let Some(first) = input.first.take() {
                return Some(first);
            }
            let variables = input.variables?;
            let variable = GenericContext::variable_at_in(variables, input.cursor);
            if variable.is_some() {
                input.cursor += 1;
            }
            variable
        }).await
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        ContextConstructionEffects::new_variables(self.0, lower_bound).await
    }

    async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<BoundTypeVarIdentity<'db>> {
        ContextConstructionEffects::identity(self.0, variable).await
    }

    async fn insert(&self, variables: &mut ContextVariables<'db>, identity: BoundTypeVarIdentity<'db>, variable: BoundTypeVarInstance<'db>) -> RunResult<()> {
        ContextConstructionEffects::insert(self.0, variables, identity, variable).await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        ContextConstructionEffects::shrink(self.0, variables).await
    }

    async fn intern(&self, program: Program<'db>, variables: ContextVariables<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::intern(self.0, program, variables).await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::publish(self.0, context).await
    }
}
