//! Passive memo ownership for canonical known-class lookup keys.

use salsa::execution_probe::{FixedQueryKeyProfile, RegistryBuilder, RunResult};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{KnownClassArgument, known_class_to_class_literal, known_class_to_instance};
use crate::Db;

impl FiniteInternedConfiguration for KnownClassArgument<'static> {
    fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> {
        // The key contains one enum discriminant and one program handle.
        Some(2)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) type KnownClassMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::class::KnownClassArgument<'static>,
        crate::types::class::KnownClassToClassLiteralConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::class::KnownClassArgument<'static>,
        crate::types::class::KnownClassToInstanceConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
);

pub(in crate::types) fn register_known_class_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::class::KnownClassArgument<'static>,
        crate::types::class::known::runtime::KnownClassMemoSchema<'db>,
    >,
> {
    let owner = KnownClassArgument::ingredient(db.zalsa());
    let literal = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        known_class_to_class_literal::fn_ingredient_(db, db.zalsa()),
    )?;
    let instance = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        known_class_to_instance::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(owner, (literal, instance))
}
