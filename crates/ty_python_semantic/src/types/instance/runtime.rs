//! Admitted canonical storage for the explicit-`Any` bit on nominal instance classes.

use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::ExplicitAnyInstanceClass;
use crate::Db;

impl FiniteInternedConfiguration for ExplicitAnyInstanceClass<'static> {
    fn field_work(_: &Self::Fields<'_>) -> Option<usize> {
        // The field contains one class discriminant and one interned identity. Hashing,
        // comparison and retirement inspect those scalars, without traversing class metadata.
        Some(8)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

/// Registers the existing explicit-`Any` class identity for admitted interning.
pub(in crate::types) fn register_explicit_any_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, ExplicitAnyInstanceClass<'static>, ()>> {
    registry.finite_interned_values_with_memos(ExplicitAnyInstanceClass::ingredient(db.zalsa()), ())
}
