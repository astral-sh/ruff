use salsa::execution_probe::{
    CallableRouteProvider, ExecutionWork, NativeValueOperation, NativeValueQuote, RetainedInput,
    RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;

use super::{SourceAccess, SourceEffects, create_source_access};
use crate::types::overrides::VariableKind;
use crate::types::overrides::runtime::{
    EffectiveVariableKindConfiguration, EffectiveVariableKindProfile,
    FunctionDefinitionConfiguration, FunctionDefinitionProfile,
};
use crate::{Db, Program};

pub(super) struct EffectiveVariableKindProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for EffectiveVariableKindProvider<'db, MakeAccess>
where
    C: EffectiveVariableKindConfiguration,
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
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let requested_bytes = size_of::<NativeValueQuote>()
                    .checked_add(size_of::<RunResult<NativeValueQuote>>())
                    .ok_or(RunError::Contract("native quote representation overflow"))?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                endpoint.check_completion()
            })
            .await;
        match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                Ok(EffectiveVariableKindProfile::input_conversion_quote())
            }
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                Err(RunError::Contract(
                    "effective variable kind input requires a retained argument tuple",
                ))
            }
            NativeValueOperation::Comparison { .. } => {
                Ok(EffectiveVariableKindProfile::output_comparison_quote())
            }
        }
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (class, name): C::Input<'db>,
    ) -> RunResult<Option<VariableKind>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_effective_variable_kind(class, &name)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<Option<VariableKind>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        effects.check_override_class_program(input.0).await?;
        admit_cycle_input::<C>(&endpoint).await?;
        // initialize_value retains its factory outside the admission callback, so refusal
        // keeps the captured Name alive until queued child tasks have drained.
        effects
            .initialize_value(|| C::cycle_initial(db, id, input))
            .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<VariableKind>,
        value: Option<VariableKind>,
        input: C::Input<'db>,
    ) -> RunResult<Option<VariableKind>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        effects.check_override_class_program(input.0).await?;
        admit_cycle_input::<C>(&endpoint).await?;
        effects
            .initialize_value(|| C::recover_from_cycle(db, cycle, last, value, input))
            .await
    }
}

pub(super) struct FunctionDefinitionProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for FunctionDefinitionProvider<'db, MakeAccess>
where
    C: FunctionDefinitionConfiguration,
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
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let requested_bytes = size_of::<NativeValueQuote>()
                    .checked_add(size_of::<RunResult<NativeValueQuote>>())
                    .ok_or(RunError::Contract("native quote representation overflow"))?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                endpoint.check_completion()
            })
            .await;
        match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                Ok(FunctionDefinitionProfile::input_conversion_quote())
            }
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => Err(
                RunError::Contract("function definition input requires a retained argument tuple"),
            ),
            NativeValueOperation::Comparison { .. } => {
                Ok(FunctionDefinitionProfile::output_comparison_quote())
            }
        }
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (scope, symbol): C::Input<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_is_function_definition(scope, symbol)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        effects.check_override_scope_program(input.0).await?;
        admit_cycle_input::<C>(&endpoint).await?;
        effects
            .initialize_value(|| C::cycle_initial(db, id, input))
            .await
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call bool,
        value: bool,
        input: C::Input<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let effects = SourceEffects::new(&access, self.program);
        effects.check_override_scope_program(input.0).await?;
        admit_cycle_input::<C>(&endpoint).await?;
        effects
            .initialize_value(|| C::recover_from_cycle(db, cycle, last, value, input))
            .await
    }
}

async fn admit_cycle_input<'run, 'db: 'run, C: Configuration>(
    endpoint: &TaskEndpoint<'run, 'db>,
) -> RunResult<()> {
    endpoint
        .local_call(|| {
            endpoint.admit_work(2)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<C::Input<'db>>(),
            })?;
            endpoint.check_completion()
        })
        .await;
    Ok(())
}
