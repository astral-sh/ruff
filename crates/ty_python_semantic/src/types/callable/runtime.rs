//! Canonical callable values retain the complete signature payload in the existing interner.

use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::CallableType;
use crate::Db;

impl FiniteInternedConfiguration for CallableType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        fields.0.field_work()?.checked_add(2)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        fields
            .0
            .field_work_bounded(fuel)?
            .checked_add(2)
            .ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) fn register_callable_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, CallableType<'static>, ()>> {
    registry.finite_interned_values_with_memos(CallableType::ingredient(db.zalsa()), ())
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::name::Name;
    use salsa::attempt_probe::AttemptOutcome;
    use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::Type;
    use crate::types::callable::CallableTypeKind;
    use crate::types::constraints::OwnedConstraintSet;
    use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};

    #[test]
    fn callable_schema_registers_without_executing_queries() {
        let mut db = setup_db();
        db.clear_salsa_events();
        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let _values = register_callable_values(&db, &mut registry)?;
                registry.seal()?.run(|_| async { Ok(()) })
            },
        );
        assert!(matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))));
        assert!(
            db.take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }

    #[test]
    fn callable_values_preserve_complete_overload_payloads() -> anyhow::Result<()> {
        let db = setup_db();
        let signature = Signature::new(
            Parameters::standard([Parameter::positional_or_keyword(Name::new("value"))
                .with_default_type(Type::bool_literal(true))]),
            Type::bool_literal(false),
        )
        .with_source_overload_index(Some(2))
        .with_probe_receiver_constraints(OwnedConstraintSet::always());
        let signatures = CallableSignature::from_overloads([
            signature.clone(),
            signature.with_return_type(Type::bool_literal(true)),
        ]);
        assert!(
            signatures
                .clone_requested_bytes()
                .is_some_and(|bytes| { bytes > 2 * size_of::<Signature<'_>>() })
        );
        let fields = (signatures, CallableTypeKind::StaticMethodLike, None);
        assert!(CallableType::field_work(&fields).is_some_and(|work| work > 2));
        let expected_fields = fields.clone();
        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let callables = register_callable_values(&db, &mut registry)?;
                registry.seal()?.run(|endpoint| async move {
                    Ok(endpoint.intern_value(&callables, fields).await)
                })
            },
        );
        let Ok(AttemptOutcome::Complete(Ok(actual))) = outcome else {
            anyhow::bail!("callable interning did not complete: {outcome:?}");
        };
        let expected = CallableType::new_internal(
            &db,
            expected_fields.0,
            expected_fields.1,
            expected_fields.2,
        );
        assert_eq!(actual, expected);
        Ok(())
    }
}
