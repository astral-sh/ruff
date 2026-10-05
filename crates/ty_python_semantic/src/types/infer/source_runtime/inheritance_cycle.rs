use salsa::execution_probe::{
    CallableRouteProvider, ExecutionWork, NativeValueOperation, NativeValueQuote, RetainedInput,
    RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;

use super::native_values::DirectInput;
use super::{SourceAccess, SourceEffects, create_source_access};
use crate::types::class::StaticClassLiteral;
use crate::types::class::static_literal::InheritanceCycle;
use crate::{Db, Program};

pub(super) trait InheritanceCycleConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = StaticClassLiteral<'a>,
        Output<'a> = Option<InheritanceCycle>,
    >
{
}

impl<C> InheritanceCycleConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<InheritanceCycle>,
        >
{
}

pub(super) struct InheritanceCycleProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

fn input_conversion_quote() -> NativeValueQuote {
    NativeValueQuote {
        work: <StaticClassLiteral<'_> as DirectInput>::CONVERSION_WORK,
        requested_bytes: size_of::<StaticClassLiteral<'_>>(),
        cleanup_work: 0,
    }
}

fn output_comparison_quote() -> NativeValueQuote {
    NativeValueQuote {
        work: 2,
        requested_bytes: size_of::<bool>(),
        cleanup_work: 0,
    }
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for InheritanceCycleProvider<'db, MakeAccess>
where
    C: InheritanceCycleConfiguration,
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
        Ok(endpoint
            .local_call(|| {
                #[cfg(test)]
                super::tests::inheritance_cycle::observe_native(_db);
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: transfer_bytes::<NativeValueQuote>(2)?,
                })?;
                endpoint.check_completion()?;
                match operation {
                    NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                        Ok(input_conversion_quote())
                    }
                    NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                        Err(RunError::Contract(
                            "inheritance cycle input requires generated handle conversion",
                        ))
                    }
                    NativeValueOperation::Comparison { .. } => Ok(output_comparison_quote()),
                }
            })
            .await)
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        #[cfg(test)]
        super::tests::inheritance_cycle::observe_body(_db, class);
        SourceEffects::new(&access, self.program)
            .infer_inheritance_cycle(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .check_inheritance_cycle_program(class)
            .await?;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: cycle_transfer_bytes(2)?,
                })?;
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
        last: &'call Option<InheritanceCycle>,
        value: Option<InheritanceCycle>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .check_inheritance_cycle_program(class)
            .await?;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(6)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: cycle_transfer_bytes(3)?,
                })?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}

fn transfer_bytes<T>(copies: usize) -> RunResult<usize> {
    size_of::<T>()
        .checked_mul(copies)
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>()))
        .ok_or(RunError::Contract(
            "inheritance cycle transfer quotation overflow",
        ))
}

fn cycle_transfer_bytes(copies: usize) -> RunResult<usize> {
    transfer_bytes::<Option<InheritanceCycle>>(copies)?
        .checked_add(size_of::<StaticClassLiteral<'_>>())
        .ok_or(RunError::Contract(
            "inheritance cycle transfer quotation overflow",
        ))
}
