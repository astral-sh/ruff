//! Retains the ordinary disjointness visitor while its controlled children execute.

use std::future::Future;

use salsa::execution_probe::{RunError, RunResult};

use super::RelationSourceEffects;
use super::guard_control::{SourceGuardControl, guard_result};
use crate::Db;
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::{CycleDetectorLookup, CycleDetectorVisit};
use crate::types::relation::DisjointnessChecker;

#[cfg(test)]
pub(super) mod tests;

pub(super) async fn with_guard<'run, 'db: 'run, 'c, E, F>(
    db: &'db dyn Db,
    checker: &DisjointnessChecker<'_, 'c, 'db>,
    left: Type<'db>,
    right: Type<'db>,
    effects: &E,
    work: impl FnOnce() -> F,
) -> RunResult<ConstraintSet<'db, 'c>>
where
    E: RelationSourceEffects<'run, 'db>,
    F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
{
    let endpoint = effects.endpoint();
    let lookup = endpoint
        .local_call(|| {
            endpoint.admit_work(1)?;
            endpoint.check_completion()?;
            Ok(checker.disjointness_visitor.lookup_visit_admitted(
                db,
                (left, right),
                &mut SourceGuardControl { endpoint },
            ))
        })
        .await;
    let mut scope = match guard_result(lookup, effects).await? {
        CycleDetectorLookup::Cached(cached) => return Ok(cached.into_result()),
        CycleDetectorLookup::Visit(CycleDetectorVisit::Ready(result)) => return Ok(result),
        CycleDetectorLookup::Visit(CycleDetectorVisit::Cycle(_)) => {
            return Ok(endpoint
                .local_call(|| {
                    endpoint.admit_work(size_of::<ConstraintSet<'db, 'c>>())?;
                    endpoint.check_completion()?;
                    Ok(ConstraintSet::from_bool(checker.constraints, false))
                })
                .await);
        }
        CycleDetectorLookup::Visit(CycleDetectorVisit::Pending(scope)) => scope,
    };
    // Keep the active entry outside the child callback so interruption drains children first.
    let result = endpoint.child_call(work).await;
    let prepared = endpoint
        .local_call(|| {
            endpoint.admit_work(1)?;
            endpoint.check_completion()?;
            Ok(scope.prepare_finish_admitted(&result, &mut SourceGuardControl { endpoint }))
        })
        .await;
    let prepared = guard_result(prepared, effects).await?;
    Ok(endpoint
        .local_call(|| {
            endpoint.admit_work(1)?;
            endpoint.check_completion()?;
            scope
                .commit_prepared_admitted(prepared, result, ConstraintSet::has_same_identity)
                .map_err(|_| RunError::Contract("disjointness visitor changed before completion"))
        })
        .await)
}
