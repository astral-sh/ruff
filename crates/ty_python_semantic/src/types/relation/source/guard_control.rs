use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::{RelationSourceEffects, RelationSourceOperation};
use crate::Db;
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::identity::{
    IDENTITY_MODE_ADMISSION_WORK, IDENTITY_MODE_WORK, admit_candidate_step, field_free_candidate,
    identity_mode_admission_bytes, identity_mode_bytes,
};
use crate::types::cyclic::{
    CycleGuardControl, HasIdentity, RelationGuardError, RelationGuardWork, cycle_cache_scan_slots,
    cycle_type_has_fixed_cost,
};
use crate::types::relation::guard::RelationKey;

type DisjointKey<'db> = (Type<'db>, Type<'db>);

pub(super) enum GuardFailure {
    Runtime(RunError),
    Identity,
}

impl From<RunError> for GuardFailure {
    fn from(error: RunError) -> Self {
        Self::Runtime(error)
    }
}

pub(super) struct SourceGuardControl<'endpoint, 'run, 'db: 'run> {
    pub(super) endpoint: &'endpoint TaskEndpoint<'run, 'db>,
}

impl<'db> SourceGuardControl<'_, '_, 'db> {
    fn admit_for<K: HasIdentity<'db>>(&mut self, work: RelationGuardWork) -> Result<(), GuardFailure> {
        let units = match work {
            RelationGuardWork::CacheAccess { capacity, probes } => {
                cycle_cache_scan_slots::<GuardFailure>(capacity)
                    .ok()
                    .and_then(|slots| slots.checked_mul(probes))
            }
            RelationGuardWork::ExactScan { len } => {
                self.endpoint.admit_work(const { IDENTITY_MODE_ADMISSION_WORK + IDENTITY_MODE_WORK })?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: const {
                        identity_mode_admission_bytes::<Result<(), GuardFailure>>()
                            + identity_mode_bytes::<K::Id>()
                    },
                })?;
                len.checked_mul(size_of::<K>() * 2)
                    .and_then(|work| work.checked_add(1))
            }
            RelationGuardWork::CandidateScan { len } => len
                .checked_mul(size_of::<K>() * 2)
                .and_then(|work| work.checked_add(1)),
            RelationGuardWork::Candidate => {
                admit_candidate_step::<K, Result<bool, GuardFailure>>(self.endpoint)?;
                return Ok(());
            }
            RelationGuardWork::Identity
            | RelationGuardWork::KeyCheck => Some(size_of::<K>() * 2),
            // Insertion prepays removing the active entry if its suspended parent is dropped.
            RelationGuardWork::ActivePush | RelationGuardWork::Finish => {
                Some(size_of::<K>() * 3 + size_of::<ConstraintSet<'_, '_>>() * 2)
            }
            RelationGuardWork::CacheKeyScan { capacity } => {
                cycle_cache_scan_slots::<GuardFailure>(capacity).ok()
            }
            RelationGuardWork::Relocate { plan } => plan.relocation_units.checked_add(
                plan.requested_payload_bytes
                    .checked_mul(2)
                    .ok_or(RunError::Contract("cycle visitor disposal overflow"))?,
            ),
            RelationGuardWork::Resource { requested_bytes } => {
                self.endpoint
                    .admit(ExecutionWork::Resource { requested_bytes })?;
                self.endpoint.check_completion()?;
                return Ok(());
            }
        }
        .ok_or(RunError::Contract("cycle visitor work overflow"))?;
        self.endpoint.admit_work(units)?;
        self.endpoint.check_completion()?;
        Ok(())
    }
}

fn identity_needs_fields(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::FunctionLiteral(_)
            | Type::NewTypeInstance(_)
            | Type::ProtocolInstance(_)
            | Type::TypeAlias(_)
            | Type::TypedDict(_)
            | Type::Recursive(_)
    )
}

fn candidate_types<'db>(
    endpoint: &TaskEndpoint<'_, 'db>,
    item: DisjointKey<'db>,
    active: DisjointKey<'db>,
) -> Result<bool, GuardFailure> {
    Ok(field_free_candidate(endpoint, &item.0, &active.0)?.ok_or(GuardFailure::Identity)?
        && field_free_candidate(endpoint, &item.1, &active.1)?.ok_or(GuardFailure::Identity)?)
}

fn identity_types<'db>(
    db: &'db dyn Db,
    item: DisjointKey<'db>,
) -> Result<<DisjointKey<'db> as HasIdentity<'db>>::Id, GuardFailure> {
    if identity_needs_fields(item.0) || identity_needs_fields(item.1) {
        return Err(GuardFailure::Identity);
    }
    Ok(item.to_identity(db))
}

impl<'db> CycleGuardControl<'db, DisjointKey<'db>> for SourceGuardControl<'_, '_, 'db> {
    type Error = GuardFailure;

    fn admit(&mut self, work: RelationGuardWork) -> Result<(), GuardFailure> {
        self.admit_for::<DisjointKey<'db>>(work)
    }

    fn key_has_fixed_cost(key: &DisjointKey<'db>) -> bool {
        cycle_type_has_fixed_cost(key.0) && cycle_type_has_fixed_cost(key.1)
    }

    fn candidate(
        &mut self,
        _db: &'db dyn Db,
        item: &DisjointKey<'db>,
        active: &DisjointKey<'db>,
    ) -> Result<bool, GuardFailure> {
        candidate_types(self.endpoint, *item, *active)
    }

    fn identity(
        &mut self,
        db: &'db dyn Db,
        item: &DisjointKey<'db>,
    ) -> Result<<DisjointKey<'db> as HasIdentity<'db>>::Id, GuardFailure> {
        identity_types(db, *item)
    }
}

impl<'db> CycleGuardControl<'db, RelationKey<'db>> for SourceGuardControl<'_, '_, 'db> {
    type Error = GuardFailure;

    fn admit(&mut self, work: RelationGuardWork) -> Result<(), GuardFailure> {
        self.admit_for::<RelationKey<'db>>(work)
    }

    fn key_has_fixed_cost(key: &RelationKey<'db>) -> bool {
        cycle_type_has_fixed_cost(key.0) && cycle_type_has_fixed_cost(key.1)
    }

    fn candidate(
        &mut self,
        _db: &'db dyn Db,
        item: &RelationKey<'db>,
        active: &RelationKey<'db>,
    ) -> Result<bool, GuardFailure> {
        Ok(candidate_types(self.endpoint, (item.0, item.1), (active.0, active.1))?
            && item.2 == active.2
            && item.3 == active.3)
    }

    fn identity(
        &mut self,
        db: &'db dyn Db,
        item: &RelationKey<'db>,
    ) -> Result<<RelationKey<'db> as HasIdentity<'db>>::Id, GuardFailure> {
        let (source, target) = identity_types(db, (item.0, item.1))?;
        Ok((source, target, item.2, item.3))
    }
}

pub(super) async fn guard_result<'run, 'db: 'run, E: RelationSourceEffects<'run, 'db>, T>(
    result: Result<T, RelationGuardError<GuardFailure>>,
    effects: &E,
) -> RunResult<T> {
    match result {
        Ok(result) => Ok(result),
        Err(RelationGuardError::Refused(GuardFailure::Runtime(error))) => Err(error),
        Err(RelationGuardError::Refused(GuardFailure::Identity)) => {
            effects
                .unavailable(RelationSourceOperation::GuardIdentity)
                .await
        }
        Err(RelationGuardError::UnsupportedKey) => {
            effects.unavailable(RelationSourceOperation::GuardKey).await
        }
        Err(RelationGuardError::CapacityExhausted) => {
            Err(RunError::Contract("cycle visitor capacity overflow"))
        }
        Err(RelationGuardError::Changed) => {
            Err(RunError::Contract("cycle visitor changed during admission"))
        }
    }
}
