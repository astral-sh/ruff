//! Passive payload accounting for bound-method identities and their callable memo.

use salsa::execution_probe::{PassiveMemoProfile, RegistryBuilder, RunResult};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{BoundMethodReceiver, BoundMethodType, callables_};
use crate::Db;
use crate::types::CallableTypes;

impl FiniteInternedConfiguration for BoundMethodType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Charge one unit per fixed field (including the receiver variant) and per
        // receiver type. Inline payload bytes cover variable hash/comparison work;
        // interned handles do not visit their referenced semantic data.
        let work = 4usize.checked_add(fields.0.inline_payload_bytes())?;
        match fields.3 {
            BoundMethodReceiver::Instance(receiver) => work
                .checked_add(1)?
                .checked_add(receiver.inline_payload_bytes()),
            BoundMethodReceiver::Constrained {
                receiver,
                constraint,
            } => work
                .checked_add(2)?
                .checked_add(receiver.inline_payload_bytes())?
                .checked_add(constraint.inline_payload_bytes()),
        }
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct BoundMethodCallablesProfile;

impl<C> PassiveMemoProfile<C> for BoundMethodCallablesProfile
where
    C: for<'db> Configuration<Output<'db> = Option<CallableTypes<'db>>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        // Charge the option tag, container retirement, and each stored handle.
        // CallableTypes owns only the container and interned handles, so retirement
        // does not traverse the signatures stored behind those handles.
        match output {
            None => Some(1),
            Some(callables) => 2usize.checked_add(callables.iter().len()),
        }
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) type BoundMethodMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::BoundMethodType<'static>,
        crate::types::method::CallablesConfiguration,
        crate::types::method::runtime::BoundMethodCallablesProfile,
    >,
);

pub(in crate::types) fn register_bound_method_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::BoundMethodType<'static>,
        crate::types::method::runtime::BoundMethodMemoSchema<'db>,
    >,
> {
    let owner = BoundMethodType::ingredient(db.zalsa());
    let callables = registry.passive_memo::<_, _, BoundMethodCallablesProfile>(
        owner,
        callables_::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(owner, (callables,))
}
