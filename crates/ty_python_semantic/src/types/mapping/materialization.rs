use salsa::execution_probe::{PassiveMemoProfile, QueryKeyProfile};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};

use crate::types::{MaterializationKind, Type};
use crate::{Db, Program};

pub(in crate::types) trait MaterializationConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<
        Fields<'a> = (Type<'a>, Program<'a>, MaterializationKind),
    > + for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>
{
}

impl<C> MaterializationConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Type<'a>, Program<'a>, MaterializationKind),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>
{
}

pub(in crate::types) struct MaterializationKeyProfile;

impl<C: MaterializationConfiguration> QueryKeyProfile<C> for MaterializationKeyProfile {
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

impl<C: MaterializationConfiguration> PassiveMemoProfile<C> for MaterializationKeyProfile {
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
