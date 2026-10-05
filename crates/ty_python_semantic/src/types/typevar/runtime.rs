use salsa::execution_probe::{
    FixedQueryKeyProfile, InternedValues, PassiveMemoGroup, RegistryBuilder, RunResult,
};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{
    BoundTypeVarInstance, TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation,
    TypeVarConstraints, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
    bound_typevar_default_type, lazy_bound_unchecked, lazy_constraints_unchecked,
    lazy_default_unchecked, top_materialized_upper_bound_inner,
};
use crate::Db;
use crate::types::Type;

impl FiniteInternedConfiguration for TypeVarIdentity<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        fields.0.len().checked_add(3)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

impl FiniteInternedConfiguration for TypeVarInstance<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        let mut work = 4usize;
        if let Some(TypeVarBoundOrConstraintsEvaluation::Eager(
            TypeVarBoundOrConstraints::UpperBound(bound),
        )) = fields.1
        {
            work = work.checked_add(bound.inline_payload_bytes())?;
        }
        if let Some(TypeVarDefaultEvaluation::Eager(default)) = fields.3 {
            work = work.checked_add(default.inline_payload_bytes())?;
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

impl FiniteInternedConfiguration for BoundTypeVarInstance<'static> {
    fn field_work(_: &Self::Fields<'_>) -> Option<usize> {
        Some(5)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

impl FiniteInternedConfiguration for TypeVarConstraints<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        constraint_field_work(&fields.0, &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        constraint_field_work(&fields.0, &mut || fuel.consume(1))
    }
}

/// Returns logical work for ordered element hashing without following canonical type handles.
/// Calls `admit` before inspecting the container and before inspecting each element.
fn constraint_field_work(
    elements: &[Type<'_>],
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

pub(in crate::types) type TypeVarConstraintsValues<'db> =
    InternedValues<'db, TypeVarConstraints<'static>, ()>;

/// Registers ordered constraint values on their existing interner, preserving duplicates.
pub(in crate::types) fn register_typevar_constraints_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<TypeVarConstraintsValues<'db>> {
    registry.finite_interned_values_with_memos(TypeVarConstraints::ingredient(db.zalsa()), ())
}

pub(in crate::types) type TypeVarIdentityValues<'db> =
    InternedValues<'db, TypeVarIdentity<'static>, ()>;

pub(in crate::types) type TypeVarInstanceValues<'db, M> =
    InternedValues<'db, TypeVarInstance<'static>, M>;

pub(in crate::types) type BoundTypeVarValues<'db, M> =
    InternedValues<'db, BoundTypeVarInstance<'static>, M>;

pub(in crate::types) fn register_typevar_identity_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<TypeVarIdentityValues<'db>> {
    registry.finite_interned_values_with_memos(TypeVarIdentity::ingredient(db.zalsa()), ())
}

pub(in crate::types) type TypeVarInstanceMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::TypeVarInstance<'static>,
            crate::types::typevar::LazyBoundUncheckedConfiguration,
            salsa::execution_probe::FixedQueryKeyProfile,
        >,
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::TypeVarInstance<'static>,
            crate::types::typevar::LazyConstraintsUncheckedConfiguration,
            salsa::execution_probe::FixedQueryKeyProfile,
        >,
    ),
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::TypeVarInstance<'static>,
            crate::types::typevar::LazyDefaultUncheckedConfiguration,
            salsa::execution_probe::FixedQueryKeyProfile,
        >,
    ),
>;

pub(in crate::types) fn register_typevar_instance_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    TypeVarInstanceValues<'db, crate::types::typevar::runtime::TypeVarInstanceMemoSchema<'db>>,
> {
    let owner = TypeVarInstance::ingredient(db.zalsa());
    let bound = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        lazy_bound_unchecked::fn_ingredient_(db, db.zalsa()),
    )?;
    let constraints = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        lazy_constraints_unchecked::fn_ingredient_(db, db.zalsa()),
    )?;
    let default = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        lazy_default_unchecked::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(
        owner,
        PassiveMemoGroup::new((bound, constraints), (default,)),
    )
}

pub(in crate::types) type BoundTypeVarMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::BoundTypeVarInstance<'static>,
        crate::types::typevar::TopMaterializedUpperBoundInnerConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::BoundTypeVarInstance<'static>,
        crate::types::typevar::BoundTypeVarDefaultTypeConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
);

pub(in crate::types) fn register_bound_typevar_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<BoundTypeVarValues<'db, crate::types::typevar::runtime::BoundTypeVarMemoSchema<'db>>>
{
    let owner = BoundTypeVarInstance::ingredient(db.zalsa());
    let bound = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        top_materialized_upper_bound_inner::fn_ingredient_(db, db.zalsa()),
    )?;
    let default = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        bound_typevar_default_type::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(owner, (bound, default))
}
