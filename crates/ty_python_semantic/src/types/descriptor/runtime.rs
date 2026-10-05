//! Passive payload accounting for implicit descriptor-call contexts and dispatch provenance.

use rustc_hash::FxHashSet;
use salsa::execution_probe::{InternedValues, PassiveMemoProfile, RegistryBuilder, RunResult};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::cyclic::descriptor_dispatch_declarations_ingredient;
use crate::types::{DescriptorDispatch, DescriptorDispatches, DescriptorGetCallContext};

impl FiniteInternedConfiguration for DescriptorGetCallContext<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Charge one unit for each fixed field and for a present instance type.
        // Inline payload bytes cover variable hash/comparison work; interned
        // handles do not visit their referenced semantic data.
        let mut work = 4usize
            .checked_add(fields.0.inline_payload_bytes())?
            .checked_add(fields.1.inline_payload_bytes())?
            .checked_add(fields.3.inline_payload_bytes())?;
        if let Some(instance) = fields.2 {
            work = work
                .checked_add(1)?
                .checked_add(instance.inline_payload_bytes())?;
        }
        Some(work)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) fn register_descriptor_get_call_context_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, DescriptorGetCallContext<'static>, ()>> {
    registry.finite_interned_values_with_memos(DescriptorGetCallContext::ingredient(db.zalsa()), ())
}

impl FiniteInternedConfiguration for DescriptorDispatch<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        Self::field_work_with(fields, &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        Self::field_work_with(fields, &mut || fuel.consume(1))
    }
}

impl DescriptorDispatch<'static> {
    fn field_work_with(
        fields: &<Self as salsa::plumbing::interned::Configuration>::Fields<'_>,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        // The signature owns shared parameter and constraint payloads. Include their
        // final-owner retirement as well as the fields visited by Hash and Eq.
        let mut work = fields
            .0
            .field_work_with(admit)?
            .max(fields.0.retirement_work_with(admit)?)
            .checked_add(4)
            .ok_or(QuoteError::Overflow)?;
        for argument in &fields.1 {
            admit()?;
            work = work
                .checked_add(1)
                .and_then(|work| work.checked_add(argument.inline_payload_bytes()))
                .ok_or(QuoteError::Overflow)?;
        }
        for comparisons in &fields.2 {
            admit()?;
            work = work.checked_add(1).ok_or(QuoteError::Overflow)?;
            for comparison in comparisons {
                admit()?;
                work = work
                    .checked_add(3)
                    .and_then(|work| {
                        work.checked_add(comparison.argument_type.inline_payload_bytes())
                    })
                    .and_then(|work| {
                        work.checked_add(comparison.parameter_type.inline_payload_bytes())
                    })
                    .ok_or(QuoteError::Overflow)?;
            }
        }
        work.checked_add(fields.3.len()).ok_or(QuoteError::Overflow)
    }
}

impl FiniteInternedConfiguration for DescriptorDispatches<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        1usize.checked_add(fields.0.len())
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct DescriptorDeclarationsProfile;

impl<C> PassiveMemoProfile<C> for DescriptorDeclarationsProfile
where
    C: for<'db> Configuration<Output<'db> = FxHashSet<Option<Definition<'db>>>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        // The declaration set stores optional interned handles, with no owned semantic payload.
        1usize.checked_add(output.len())
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) fn register_descriptor_dispatch_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, DescriptorDispatch<'static>, ()>> {
    registry.finite_interned_values_with_memos(DescriptorDispatch::ingredient(db.zalsa()), ())
}

pub(in crate::types) type DescriptorDispatchesMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::DescriptorDispatches<'static>,
        crate::types::cyclic::DeclarationsConfiguration,
        crate::types::descriptor::runtime::DescriptorDeclarationsProfile,
    >,
);

pub(in crate::types) fn register_descriptor_dispatches_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::DescriptorDispatches<'static>,
        crate::types::descriptor::runtime::DescriptorDispatchesMemoSchema<'db>,
    >,
> {
    let owner = DescriptorDispatches::ingredient(db.zalsa());
    let declarations = registry.passive_memo::<_, _, DescriptorDeclarationsProfile>(
        owner,
        descriptor_dispatch_declarations_ingredient(db),
    )?;
    registry.finite_interned_values_with_memos(owner, (declarations,))
}

#[cfg(test)]
mod tests {
    use salsa::attempt_probe::AttemptOutcome;
    use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::signatures::{CallableSignature, Signature};
    use crate::types::{DescriptorArgumentComparison, Type};

    #[test]
    fn descriptor_dispatches_schema_preserves_a_populated_declarations_memo() -> anyhow::Result<()>
    {
        let db = setup_db();
        let dispatch = DescriptorDispatch::new(
            &db,
            CallableSignature::single(Signature::unknown()),
            Box::<[Type<'_>]>::default(),
            Box::from([Box::<[DescriptorArgumentComparison<'_>]>::default()]),
            Box::from([0usize]),
            false,
        );
        let elements = Box::<[DescriptorDispatch<'_>]>::from([dispatch]);
        let expected = DescriptorDispatches::new(&db, elements.clone());
        let declarations = FxHashSet::from_iter([None]);
        assert_eq!(expected.declarations(&db), &declarations);
        let mut reader = db.clone();
        reader.clear_salsa_events();

        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let values = register_descriptor_dispatches_values(&db, &mut registry)?;
                registry.seal()?.run(|endpoint| async move {
                    Ok(endpoint.intern_value(&values, (elements,)).await)
                })
            },
        );
        let Ok(AttemptOutcome::Complete(Ok(actual))) = outcome else {
            anyhow::bail!("descriptor dispatch-list interning did not complete: {outcome:?}");
        };
        assert_eq!(actual, expected);
        assert_eq!(actual.declarations(&db), &declarations);
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
        Ok(())
    }
}
