//! Registration profiles for the existing canonical constructor `__new__` query.

use salsa::execution_probe::{
    CallableRouteProvider, ExecutionWork, NativeValueOperation, NativeValueQuote, PassiveMemoProfile,
    QueryKeyProfile, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{SourceAccess, SourceEffects, create_source_access};
use crate::place::PlaceAndQualifiers;
use crate::types::Type;
use crate::{Db, Program};

const OPTIONAL_MEMBER_COMPARISON_FIELDS: usize = 17;

/// Identifies the argument and result shapes of the nested `lookup_dunder_new_inner` query.
pub(super) trait ConstructorNewConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Type<'a>)>
    + for<'a> Configuration<DbView = dyn Db, Output<'a> = Option<PlaceAndQualifiers<'a>>>
{
}

impl<C> ConstructorNewConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Type<'a>)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = Option<PlaceAndQualifiers<'a>>>
{
}

/// Quotes hashing and equality of the canonical `(program, type)` key and passive memo cleanup.
#[derive(Debug)]
pub(super) struct ConstructorNewKeyProfile;

impl<C: ConstructorNewConfiguration> QueryKeyProfile<C> for ConstructorNewKeyProfile {
    fn input_work<'db>(
        fields: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        3usize.checked_add(fields.1.inline_payload_bytes())
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

impl<C: ConstructorNewConfiguration> PassiveMemoProfile<C> for ConstructorNewKeyProfile {
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

/// Resolves constructor members through the source run's existing access and query lifecycle.
#[derive(Debug)]
pub(super) struct ConstructorNewProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for ConstructorNewProvider<'db, MakeAccess>
where
    C: ConstructorNewConfiguration,
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
        let (work, requested_bytes) = match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                let bytes = size_of::<(Program<'db>, Type<'db>)>()
                    .checked_mul(2)
                    .ok_or(RunError::Contract("constructor new input byte quote overflow"))?;
                (4, bytes)
            }
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "constructor new input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                let work = endpoint
                    .local_call(|| {
                        endpoint.admit_work(OPTIONAL_MEMBER_COMPARISON_FIELDS)?;
                        endpoint.check_completion()?;
                        let left_text_work = left
                            .and_then(|member| member.place.raw_type())
                            .map(Type::inline_payload_bytes)
                            .unwrap_or(0);
                        let right_text_work = right
                            .and_then(|member| member.place.raw_type())
                            .map(Type::inline_payload_bytes)
                            .unwrap_or(0);
                        OPTIONAL_MEMBER_COMPARISON_FIELDS
                            .checked_add(left_text_work)
                            .and_then(|work| work.checked_add(right_text_work))
                            .ok_or(RunError::Contract("constructor new comparison work overflow"))
                    })
                    .await;
                (work, 0)
            }
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (program, ty): C::Input<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                if program != self.program {
                    return Err(RunError::Contract("constructor new program is foreign"));
                }
                Ok(())
            })
            .await;
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, program)
            .infer_constructor_new(ty)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                let bytes = size_of::<C::Input<'db>>()
                    .checked_add(size_of::<C::Output<'db>>())
                    .ok_or(RunError::Contract("constructor new initial byte quote overflow"))?;
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes: bytes })?;
                endpoint.check_completion()?;
                if input.0 != self.program {
                    return Err(RunError::Contract("constructor new program is foreign"));
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
        last: &'call Option<PlaceAndQualifiers<'db>>,
        value: Option<PlaceAndQualifiers<'db>>,
        input: C::Input<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                let bytes = size_of::<C::Output<'db>>()
                    .checked_mul(2)
                    .and_then(|outputs| size_of::<C::Input<'db>>().checked_add(outputs))
                    .ok_or(RunError::Contract("constructor new recovery byte quote overflow"))?;
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes: bytes })?;
                endpoint.check_completion()?;
                if input.0 != self.program {
                    return Err(RunError::Contract("constructor new program is foreign"));
                }
                Ok(C::recover_from_cycle(db, cycle, last, value, input))
            })
            .await)
    }
}
