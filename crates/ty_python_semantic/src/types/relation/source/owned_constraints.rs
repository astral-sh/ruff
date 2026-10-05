//! Owned lazy-assignability results retain the original comparison owners through terminal saving.

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::retained::RetainedChecker;
use super::{RelationSourceEffects, RelationSourceOperation, RetainedRelationSource};
use crate::Db;
use crate::types::Type;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::relation::{
    OwnedRelationKind, OwnedRelationProducerEffects, owned_relation_constraints_with,
};

/// Supplies the shared producer with its retained lazy checker and queued source access.
struct AssignabilityProducer<'effect, 'run, 'pool, 'owner, 'c, 'db, R> {
    db: &'db dyn Db,
    endpoint: &'effect TaskEndpoint<'run, 'db>,
    checker: RetainedChecker<'pool, 'owner, 'c, 'db>,
    access: R,
}

impl<'run, 'pool: 'run, 'owner: 'pool, 'c: 'owner, 'db: 'c, R: RetainedRelationSource<'run, 'db>>
    OwnedRelationProducerEffects<'c, 'db>
    for AssignabilityProducer<'_, 'run, 'pool, 'owner, 'c, 'db, R>
{
    type Error = RunError;

    async fn assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let access =
            local_with_fixed_transfers_at(self.endpoint, 2, 0, || self.access.clone()).await?;
        self.checker
            .pair_with_fixed_transfers(self.db, self.endpoint, access, source, target)
            .await
    }

    async fn equivalent(
        &self,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let effects =
            local_with_fixed_transfers_at(self.endpoint, 1, 0, || self.access.effects()).await?;
        effects
            .unavailable(RelationSourceOperation::OwnedEquivalence)
            .await
    }
}

/// Runs the shared owned-assignability producer without reducing its condition to a boolean.
pub(super) async fn produce<
    'run,
    'pool: 'run,
    'owner: 'pool,
    'c: 'owner,
    'db: 'c,
    R: RetainedRelationSource<'run, 'db>,
>(
    db: &'db dyn Db,
    endpoint: &TaskEndpoint<'run, 'db>,
    checker: RetainedChecker<'pool, 'owner, 'c, 'db>,
    access: R,
    source: Type<'db>,
    target: Type<'db>,
) -> RunResult<ConstraintSet<'db, 'c>> {
    let producer = local_with_fixed_transfers_at(endpoint, 3, 0, || AssignabilityProducer {
        db,
        endpoint,
        checker,
        access,
    })
    .await?;
    let result = owned_relation_constraints_with(
        OwnedRelationKind::Assignability,
        source,
        target,
        &producer,
    )
    .await?;
    local_with_fixed_transfers_at(endpoint, 1, 0, || result).await
}

/// Saves a terminal result while leaving its builder available to retained checkers and children.
/// A nonterminal result requires graph compaction and reports that unavailable operation.
pub(in crate::types) async fn package_terminal<
    'run,
    'db: 'run,
    'c,
    E: RelationSourceEffects<'run, 'db>,
>(
    builder: &'c ConstraintSetBuilder<'db>,
    value: ConstraintSet<'db, 'c>,
    effects: &E,
) -> RunResult<OwnedConstraintSet<'db>> {
    #[cfg(test)]
    super::receiver_constraint_observations::observe_before(
        super::receiver_constraint_observations::Stage::Package,
    );
    // The terminal has no backing allocation; this includes its eventual destruction.
    let owned = local_with_fixed_transfers_at(effects.endpoint(), 5, 0, || {
        if !value.is_from_builder(builder) {
            return Err(RunError::Contract(
                "owned constraint belongs to another builder",
            ));
        }
        let owned = value.to_owned_terminal();
        #[cfg(test)]
        if owned.is_some() {
            super::receiver_constraint_observations::observe_after(
                super::receiver_constraint_observations::Stage::Package,
            );
        }
        Ok(owned)
    })
    .await??;
    let Some(owned) = owned else {
        return effects
            .unavailable(RelationSourceOperation::OwnedCompaction)
            .await;
    };
    local_with_fixed_transfers_at(effects.endpoint(), 1, 0, || owned).await
}
