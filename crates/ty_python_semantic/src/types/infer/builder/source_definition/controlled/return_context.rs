//! Reuse ordered generic-context construction for renamed, inherited, and retained variables.

use std::marker::PhantomData;

use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::FixedFieldCopy;
use super::storage::slots;
use super::{SourceAccess, SourceEffects};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::generics::context_construction::{
    ContextConstructionEffects, ContextVariables, context_from_typevars_with,
};
use crate::types::mapping::return_callables::RetainedReturnTypevars;
use crate::types::{BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext};
use crate::{Program, ProgramEnvironment};

/// Borrows variables in their ordinary insertion order without copying a context's stored map.
#[derive(Clone, Copy, Debug)]
enum ContextValues<'run, 'db> {
    Renamed(RetainedReturnTypevars<'run, 'db>),
    Stored {
        first: &'db ContextVariables<'db>,
        second: Option<&'db ContextVariables<'db>>,
    },
}

/// Tracks filtering and one-item lookahead while shared context construction consumes a view.
#[derive(Debug)]
struct ReturnContextInput<'data, 'run, 'db> {
    values: ContextValues<'run, 'db>,
    cursor: usize,
    excluded: Option<&'data FxHashSet<BoundTypeVarInstance<'db>>>,
    pending: Option<BoundTypeVarInstance<'db>>,
}

/// Adapts borrowed return-scope inputs to the existing context allocation and interning effects.
struct ReturnContextEffects<'source, 'data, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    input: PhantomData<&'data ()>,
}

impl<'data, 'run, 'db: 'run + 'data, A: SourceAccess<'run, 'db>> ContextConstructionEffects<'db>
    for ReturnContextEffects<'_, 'data, '_, 'run, 'db, A>
{
    type Error = RunError;
    type Input = ReturnContextInput<'data, 'run, 'db>;

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        ContextConstructionEffects::program(self.source, env).await
    }

    async fn input_lower_bound(&self, input: &Self::Input) -> RunResult<usize> {
        self.source
            .local_with_fixed_transfers(8, 0, || {
                if input.excluded.is_some() {
                    return Ok(0);
                }
                match input.values {
                    ContextValues::Renamed(values) => Ok(values.len()),
                    ContextValues::Stored { first, second } => first
                        .len()
                        .checked_add(second.map(ContextVariables::len).unwrap_or(0))
                        .ok_or(RunError::Contract("return context length overflow")),
                }
            })
            .await?
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        loop {
            let variable = self
                .source
                .local_with_fixed_transfers(16, 0, || {
                    if let Some(variable) = input.pending.take() {
                        return Some(variable);
                    }
                    let variable = match input.values {
                        ContextValues::Renamed(values) => values.value_at(input.cursor),
                        ContextValues::Stored { first, second } => {
                            if input.cursor < first.len() {
                                GenericContext::variable_at_in(first, input.cursor)
                            } else {
                                second.and_then(|second| {
                                    GenericContext::variable_at_in(
                                        second,
                                        input.cursor - first.len(),
                                    )
                                })
                            }
                        }
                    };
                    if variable.is_some() {
                        input.cursor += 1;
                    }
                    variable
                })
                .await?;
            let Some(variable) = variable else {
                return Ok(None);
            };
            let Some(excluded) = input.excluded else {
                return Ok(Some(variable));
            };
            let work = self
                .source
                .local_with_fixed_transfers(8, 0, || {
                    slots(excluded.capacity())
                        .and_then(|slots| slots.checked_mul(8)?.checked_add(8))
                })
                .await?;
            let excluded = self
                .source
                .local_quoted_with_fixed_transfers(
                    work.map(|work| (work, 0)).ok_or(RunError::Contract(
                        "return context lookup quotation overflow",
                    )),
                    || excluded.contains(&variable),
                )
                .await?;
            if !excluded {
                return Ok(Some(variable));
            }
        }
    }

    async fn new_variables(&self, lower_bound: usize) -> RunResult<ContextVariables<'db>> {
        ContextConstructionEffects::new_variables(self.source, lower_bound).await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        ContextConstructionEffects::identity(self.source, variable).await
    }

    async fn insert(
        &self,
        variables: &mut ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        ContextConstructionEffects::insert(self.source, variables, identity, variable).await
    }

    async fn shrink(&self, variables: &mut ContextVariables<'db>) -> RunResult<()> {
        ContextConstructionEffects::shrink(self.source, variables).await
    }

    async fn intern(
        &self,
        program: Program<'db>,
        variables: ContextVariables<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.source
            .local_with_fixed_transfers(
                8,
                size_of::<ContextVariables<'db>>()
                    .checked_mul(4)
                    .ok_or(RunError::Contract("return context transfer quotation overflow"))?,
                || (),
            )
            .await?;
        self.source
            .type_parameter_future(|| {
                ContextConstructionEffects::intern(self.source, program, variables)
            })
            .await?
            .await
    }

    async fn publish(&self, context: GenericContext<'db>) -> RunResult<GenericContext<'db>> {
        ContextConstructionEffects::publish(self.source, context).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs a context from an admitted borrowed input, preserving insertion and deduplication order.
    async fn return_context_from_input(
        &self,
        input: ReturnContextInput<'_, 'run, 'db>,
    ) -> RunResult<GenericContext<'db>> {
        let env = self
            .local_with_fixed_transfers(2, 0, || ProgramEnvironment::from_program(self.program))
            .await?;
        let effects = self
            .local_with_fixed_transfers(2, 0, || ReturnContextEffects {
                source: self,
                input: PhantomData,
            })
            .await?;
        self.type_parameter_future(|| context_from_typevars_with(self.db(), &env, input, &effects))
            .await?
            .await
    }

    /// Builds the callable context from renamed variables in the replacement map's insertion order.
    pub(super) async fn renamed_return_context(
        &self,
        values: RetainedReturnTypevars<'run, 'db>,
    ) -> RunResult<GenericContext<'db>> {
        let input = self
            .local_with_fixed_transfers(5, 0, || ReturnContextInput {
                values: ContextValues::Renamed(values),
                cursor: 0,
                excluded: None,
                pending: None,
            })
            .await?;
        self.type_parameter_future(|| self.return_context_from_input(input)).await?.await
    }

    /// Appends inherited variables after an overload's existing variables using ordinary identity deduplication.
    pub(super) async fn merge_return_context(
        &self,
        existing: GenericContext<'db>,
        inherited: GenericContext<'db>,
    ) -> RunResult<GenericContext<'db>> {
        let existing_program = self
            .local_with_fixed_transfers(4, 0, || {
                existing
                    .field_requests(self.access.endpoint().field_request_context())
                    .program()
            })
            .await?;
        let existing_program = self
            .field_with_profile(existing_program, &FixedFieldCopy)
            .await?;
        self.check_program(existing_program)?;
        let inherited_program = self
            .local_with_fixed_transfers(4, 0, || {
                inherited
                    .field_requests(self.access.endpoint().field_request_context())
                    .program()
            })
            .await?;
        let inherited_program = self
            .field_with_profile(inherited_program, &FixedFieldCopy)
            .await?;
        self.check_program(inherited_program)?;
        let first = TypeVarBindingEffects::variables(self, existing).await?;
        let second = TypeVarBindingEffects::variables(self, inherited).await?;
        let input = self
            .local_with_fixed_transfers(6, 0, || ReturnContextInput {
                values: ContextValues::Stored {
                    first,
                    second: Some(second),
                },
                cursor: 0,
                excluded: None,
                pending: None,
            })
            .await?;
        self.type_parameter_future(|| self.return_context_from_input(input)).await?.await
    }

    /// Removes moved originals from the outer context and avoids interning an empty replacement context.
    pub(super) async fn trim_return_context(
        &self,
        context: GenericContext<'db>,
        excluded: &FxHashSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let first = TypeVarBindingEffects::variables(self, context).await?;
        let mut input = self
            .local_with_fixed_transfers(6, 0, || ReturnContextInput {
                values: ContextValues::Stored {
                    first,
                    second: None,
                },
                cursor: 0,
                excluded: Some(excluded),
                pending: None,
            })
            .await?;
        let effects = self
            .local_with_fixed_transfers(2, 0, || ReturnContextEffects {
                source: self,
                input: PhantomData,
            })
            .await?;
        let first = effects.next_variable(&mut input).await?;
        let Some(first) = first else {
            return Ok(None);
        };
        self.local_with_fixed_transfers(1, 0, || input.pending = Some(first))
            .await?;
        self.type_parameter_future(|| self.return_context_from_input(input)).await?.await.map(Some)
    }
}
