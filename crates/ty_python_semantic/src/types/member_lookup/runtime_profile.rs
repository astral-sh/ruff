//! Admission profiles for canonical member and public-place queries.

use salsa::execution_probe::{
    NativeValueOperation, NativeValueQuote, PassiveMemoProfile, QueryKeyProfile, RetainedInput,
    RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;

use crate::place::{ConsideredDefinitions, Place, PlaceAndQualifiers, RequiresExplicitReExport};
use crate::types::descriptor::DescriptorResult;
use crate::types::{MemberLookupKey, Type};
#[cfg(test)]
use crate::types::{MemberLookupResult, ResolvedMember};
use crate::{Db, Program};

pub(super) trait NativeInputProfile {
    const RETAINED_TUPLE: bool;
    const CONVERSION_WORK: usize;
}

impl NativeInputProfile for MemberLookupKey<'_> {
    const RETAINED_TUPLE: bool = false;
    const CONVERSION_WORK: usize = 1;
}

// Tuple Clone copies the generated ScopeId identity, the ScopedPlaceId index and tag, and
// the two unit enums. None of these concrete Clone implementations visits retained fields.
impl NativeInputProfile
    for (
        ScopeId<'_>,
        ScopedPlaceId,
        RequiresExplicitReExport,
        ConsideredDefinitions,
    )
{
    const RETAINED_TUPLE: bool = true;
    const CONVERSION_WORK: usize = 5;
}

// Program Clone copies its generated identity. Type's derived Clone copies finite scalar,
// enum, and handle fields; Todo labels copy a borrowed string, without visiting its bytes.
// Option<Type> adds one tag. This tuple owns no allocation requiring cleanup after conversion.
impl NativeInputProfile for (Program<'_>, Type<'_>, Option<Type<'_>>, Type<'_>) {
    const RETAINED_TUPLE: bool = true;
    const CONVERSION_WORK: usize = 8;
}

pub(super) trait NativeOutputProfile {
    const COMPARISON_FIELDS: usize;

    fn inline_payload_bytes(&self) -> usize;
}

impl NativeOutputProfile for PlaceAndQualifiers<'_> {
    const COMPARISON_FIELDS: usize = 16;

    fn inline_payload_bytes(&self) -> usize {
        match self.place {
            Place::Defined(place) => place.ty.inline_payload_bytes(),
            Place::Undefined => 0,
        }
    }
}

#[cfg(test)]
impl NativeOutputProfile for MemberLookupResult<'_> {
    const COMPARISON_FIELDS: usize = 16;

    fn inline_payload_bytes(&self) -> usize {
        match self {
            Ok(ResolvedMember::Plain(member)) => member.inline_payload_bytes(),
            Ok(ResolvedMember::WithMetadata(_)) | Err(_) => 0,
        }
    }
}

impl NativeOutputProfile for DescriptorResult<'_> {
    const COMPARISON_FIELDS: usize = 20;

    fn inline_payload_bytes(&self) -> usize {
        match self {
            Ok(Some(result)) => result.return_type.inline_payload_bytes(),
            Err(error) => error.fallback().return_type.inline_payload_bytes(),
            Ok(None) => 0,
        }
    }
}

pub(super) async fn quote_native_value<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: Configuration,
    C::Input<'db>: NativeInputProfile,
    C::Output<'db>: NativeOutputProfile,
{
    let work = match operation {
        NativeValueOperation::InputConversion(input) => {
            let retained_tuple = matches!(input, RetainedInput::Interned(_));
            if retained_tuple != <C::Input<'db> as NativeInputProfile>::RETAINED_TUPLE {
                return Err(RunError::Contract(
                    "member native input representation mismatch",
                ));
            }
            <C::Input<'db> as NativeInputProfile>::CONVERSION_WORK
        }
        NativeValueOperation::Comparison { left, right } => {
            endpoint
                .local_call(|| {
                    let fields = <C::Output<'db> as NativeOutputProfile>::COMPARISON_FIELDS;
                    endpoint.admit_work(fields)?;
                    endpoint.check_completion()?;
                    fields
                        .checked_add(left.inline_payload_bytes())
                        .and_then(|work| work.checked_add(right.inline_payload_bytes()))
                        .ok_or(RunError::Contract("member native comparison work overflow"))
                })
                .await
        }
    };
    Ok(NativeValueQuote {
        work,
        requested_bytes: 0,
        cleanup_work: 0,
    })
}

pub(in crate::types) trait PlaceConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<
        Fields<'a> = (
            ScopeId<'a>,
            ScopedPlaceId,
            RequiresExplicitReExport,
            ConsideredDefinitions,
        ),
    > + for<'a> Configuration<DbView = dyn Db, Output<'a> = PlaceAndQualifiers<'a>>
{
}

impl<C> PlaceConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (
                ScopeId<'a>,
                ScopedPlaceId,
                RequiresExplicitReExport,
                ConsideredDefinitions,
            ),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = PlaceAndQualifiers<'a>>
{
}

pub(in crate::types) async fn quote_place_native_value<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: PlaceConfiguration,
{
    quote_native_value(endpoint, operation).await
}

pub(in crate::types) trait DescriptorConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<
        Fields<'a> = (Program<'a>, Type<'a>, Option<Type<'a>>, Type<'a>),
    > + for<'a> Configuration<DbView = dyn Db, Output<'a> = DescriptorResult<'a>>
{
}

impl<C> DescriptorConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Program<'a>, Type<'a>, Option<Type<'a>>, Type<'a>),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = DescriptorResult<'a>>
{
}

pub(in crate::types) trait ClassMemberConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = MemberLookupKey<'a>,
        Output<'a> = PlaceAndQualifiers<'a>,
    >
{
}

impl<C> ClassMemberConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = PlaceAndQualifiers<'a>,
        >
{
}

pub(in crate::types) async fn quote_class_member_native_value<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: ClassMemberConfiguration,
{
    quote_native_value(endpoint, operation).await
}

pub(in crate::types) async fn quote_descriptor_native_value<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: DescriptorConfiguration,
{
    quote_native_value(endpoint, operation).await
}

pub(in crate::types) struct PlaceLookupKeyProfile;

impl<C: PlaceConfiguration> PassiveMemoProfile<C> for PlaceLookupKeyProfile {
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

impl<C: PlaceConfiguration> QueryKeyProfile<C> for PlaceLookupKeyProfile {
    fn input_work<'db>(
        _input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        Some(0)
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct DescriptorLookupKeyProfile;

impl<C: DescriptorConfiguration> PassiveMemoProfile<C> for DescriptorLookupKeyProfile {
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

impl<C: DescriptorConfiguration> QueryKeyProfile<C> for DescriptorLookupKeyProfile {
    fn input_work<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        input
            .1
            .inline_payload_bytes()
            .checked_add(input.2.map_or(0, Type::inline_payload_bytes))?
            .checked_add(input.3.inline_payload_bytes())
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}
