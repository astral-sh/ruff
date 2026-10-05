use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, PassiveMemoProfile,
    QueryKeyProfile, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::types::Type;
use crate::{Db, Program};

pub(super) trait DataDescriptorConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Type<'a>, Program<'a>, bool)>
    + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>
{
}

impl<C> DataDescriptorConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Type<'a>, Program<'a>, bool)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>
{
}

pub(super) struct DataDescriptorKeyProfile;

impl<C: DataDescriptorConfiguration> QueryKeyProfile<C> for DataDescriptorKeyProfile {
    fn input_work<'db>(
        fields: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        3usize.checked_add(fields.0.inline_payload_bytes())
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

impl<C: DataDescriptorConfiguration> PassiveMemoProfile<C> for DataDescriptorKeyProfile {
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(super) struct DataDescriptorProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for DataDescriptorProvider<'db, MakeAccess>
where
    C: DataDescriptorConfiguration,
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
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 4,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "data descriptor input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                <bool as native_values::OutputProfile<'db>>::comparison_work(endpoint, left, right)
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
        (ty, program, any_of_union): C::Input<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                if program != self.program {
                    return Err(RunError::Contract("data descriptor program is foreign"));
                }
                Ok(())
            })
            .await;
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, program)
            .infer_data_descriptor(ty, any_of_union)
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
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                if input.1 != self.program {
                    return Err(RunError::Contract("data descriptor program is foreign"));
                }
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
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
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                if input.1 != self.program {
                    return Err(RunError::Contract("data descriptor program is foreign"));
                }
                Ok(C::recover_from_cycle(db, cycle, last, value, input))
            })
            .await)
    }
}
