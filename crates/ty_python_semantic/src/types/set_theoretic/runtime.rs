//! Finite payload accounting for canonical union and intersection values.

use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{IntersectionType, UnionType};
use crate::Db;
use crate::types::Type;

impl FiniteInternedConfiguration for UnionType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Hash/Eq inspect the stored Type payloads without following interned semantic handles.
        element_field_work(fields.0.iter(), &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        element_field_work(fields.0.iter(), &mut || fuel.consume(1))
    }
}

pub(in crate::types) fn register_union_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, UnionType<'static>, ()>> {
    // cached_expand_aliases stores its memo on the generated (UnionType, Program) key interner.
    // UnionType itself has no attached query memos.
    registry.finite_interned_values_with_memos(UnionType::ingredient(db.zalsa()), ())
}

impl FiniteInternedConfiguration for IntersectionType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Both fields compare their ordered Type payloads without following semantic handles.
        element_field_work(fields.0.iter().chain(fields.1.iter()), &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        element_field_work(fields.0.iter().chain(fields.1.iter()), &mut || {
            fuel.consume(1)
        })
    }
}

fn element_field_work<'a, 'db: 'a>(
    elements: impl Iterator<Item = &'a Type<'db>>,
    admit: &mut impl FnMut() -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    admit()?;
    let mut work = 2usize;
    for element in elements {
        admit()?;
        work = work
            .checked_add(1)
            .and_then(|work| work.checked_add(element.inline_payload_bytes()))
            .ok_or(QuoteError::Overflow)?;
    }
    Ok(work)
}

pub(in crate::types) fn register_intersection_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, IntersectionType<'static>, ()>> {
    registry.finite_interned_values_with_memos(IntersectionType::ingredient(db.zalsa()), ())
}
