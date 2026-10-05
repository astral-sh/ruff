#[cfg(feature = "accumulator")]
use crate::accumulator::accumulated_map::InputAccumulatedValues;
use crate::attempt_probe::MemoReuse;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{self as transfer_trace, Event, Kind};
use crate::cycle::{CycleHeads, CycleRecoveryStrategy, IterationStamp};
use crate::function::memo::{
    MemoHeader, MemoOutputCheck, MemoVerification, TryClaimCycleHeadsIter, TryClaimHeadsResult,
};
use crate::function::{Configuration, IngredientImpl};
use std::sync::atomic::Ordering;

use crate::key::DatabaseKeyIndex;
use crate::zalsa::{Zalsa, ZalsaDatabase};
use crate::zalsa_local::{QueryRevisions, ZalsaLocal};
use crate::{Id, Revision};

pub(super) mod validation;

pub(super) enum ValidationProbe<'db> {
    Miss,
    Ready(VerifyResult),
    Verify(EagerValidationVerification<'db>),
}

pub(super) struct EagerValidationVerification<'db> {
    header: &'db MemoHeader,
    changed_after: Revision,
    verification: MemoVerification<'db>,
}

impl EagerValidationVerification<'_> {
    #[cfg(test)]
    pub(super) fn key(&self) -> DatabaseKeyIndex {
        self.verification.key()
    }

    pub(super) fn output_check_work(&self, check: MemoOutputCheck) -> usize {
        self.verification.output_check_work(check)
    }

    pub(super) fn outputs_are_empty(&self) -> bool {
        self.verification.outputs_are_empty()
    }

    pub(super) fn controlled_outputs_are_empty(&self) -> bool {
        self.verification.controlled_outputs_are_empty()
    }

    pub(super) fn event(&self) {
        self.verification.event();
    }

    pub(super) fn finish(self) -> VerifyResult {
        self.verification.publish_shallow();
        if self.header.revisions.changed_at > self.changed_after {
            VerifyResult::changed()
        } else {
            VerifyResult::unchanged_for_memo(&self.header.revisions)
        }
    }
}

/// Result of memo validation.
#[derive(Debug, Copy, Clone)]
pub enum VerifyResult {
    /// Memo has changed and needs to be recomputed.
    Changed,

    /// Memo remains valid.
    ///
    /// The inner value tracks whether the memo or any of its dependencies have an
    /// accumulated value.
    Unchanged {
        #[cfg(feature = "accumulator")]
        accumulated: InputAccumulatedValues,
    },
}

impl VerifyResult {
    pub(crate) const fn changed_if(changed: bool) -> Self {
        if changed {
            Self::changed()
        } else {
            Self::unchanged()
        }
    }

    pub(crate) const fn changed() -> Self {
        Self::Changed
    }

    pub(crate) const fn unchanged() -> Self {
        Self::Unchanged {
            #[cfg(feature = "accumulator")]
            accumulated: InputAccumulatedValues::Empty,
        }
    }

    #[inline]
    #[cfg(feature = "accumulator")]
    pub(crate) fn unchanged_with_accumulated(accumulated: InputAccumulatedValues) -> Self {
        Self::Unchanged { accumulated }
    }

    #[inline]
    #[cfg(not(feature = "accumulator"))]
    pub(crate) fn unchanged_with_accumulated() -> Self {
        Self::unchanged()
    }

    /// Returns an unchanged result that propagates accumulated values from both
    /// the memo itself and its inputs.
    #[inline]
    fn unchanged_for_memo(revisions: &QueryRevisions) -> Self {
        #[cfg(not(feature = "accumulator"))]
        let _ = revisions;

        Self::unchanged_with_accumulated(
            #[cfg(feature = "accumulator")]
            match revisions.accumulated() {
                Some(_) => InputAccumulatedValues::Any,
                None => revisions.accumulated_inputs.load(),
            },
        )
    }

    pub(crate) const fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged { .. })
    }
}

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    pub(super) fn maybe_changed_after<'db>(
        &'db self,
        db: &'db C::DbView,
        id: Id,
        revision: Revision,
    ) -> VerifyResult {
        let _operation = crate::attempt_probe::enter(db.zalsa(), C::ATTEMPT_POLICY, C::DEBUG_NAME);
        let validation = validation::Validation::new(self, db, id, revision);
        if let Some(result) = validation.probe() {
            result
        } else {
            validation.execute_cold()
        }
    }
}

impl MemoHeader {
    pub(super) fn current_revision_result(&self, revision: Revision) -> VerifyResult {
        if self.revisions.changed_at > revision {
            VerifyResult::changed()
        } else {
            VerifyResult::unchanged_for_memo(&self.revisions)
        }
    }

    fn maybe_changed_after_probe<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        database_key_index: DatabaseKeyIndex,
        revision: Revision,
        #[cfg(feature = "detailed-trace")] has_value: bool,
    ) -> ValidationProbe<'db> {
        if !matches!(self.attempt_reuse(zalsa), MemoReuse::Ordinary)
            && !self.is_finality_candidate()
        {
            return ValidationProbe::Ready(VerifyResult::changed());
        }

        let can_shallow_update = self.shallow_verify_memo(
            zalsa,
            database_key_index,
            #[cfg(feature = "detailed-trace")]
            has_value,
        );
        if can_shallow_update.yes() && !self.may_be_provisional() {
            match can_shallow_update {
                ShallowUpdate::HigherDurability => {
                    ValidationProbe::Verify(EagerValidationVerification {
                        header: self,
                        changed_after: revision,
                        verification: MemoVerification::new(
                            self,
                            zalsa,
                            database_key_index,
                            zalsa.current_revision(),
                        ),
                    })
                }
                ShallowUpdate::Verified => {
                    ValidationProbe::Ready(self.current_revision_result(revision))
                }
                ShallowUpdate::No => ValidationProbe::Miss,
            }
        } else {
            ValidationProbe::Miss
        }
    }

    /// `Some` if the memo's value and `changed_at` time is still valid in this revision.
    /// Does only a shallow O(1) check, doesn't walk the dependencies.
    ///
    /// In general, a provisional memo (from cycle iteration) does not verify. Since we don't
    /// eagerly finalize all provisional memos in cycle iteration, we have to lazily check here
    /// (via `validate_provisional`) whether a may-be-provisional memo should actually be verified
    /// final, because its cycle heads are all now final.
    #[inline]
    pub(super) fn shallow_verify_memo(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
        #[cfg(feature = "detailed-trace")] has_value: bool,
    ) -> ShallowUpdate {
        // A retained participant can belong to an accepted component even if its producer
        // later refused unrelated work. Shallow validity still requires the claimed head
        // proof before that participant can be consumed.
        if !matches!(self.attempt_reuse(zalsa), MemoReuse::Ordinary)
            && !self.is_finality_candidate()
        {
            return ShallowUpdate::No;
        }

        #[cfg(feature = "detailed-trace")]
        crate::tracing::debug!(
            "{database_key_index:?}: shallow_verify_memo(memo = {memo:#?})",
            memo = self.tracing_debug(has_value)
        );
        let verified_at = self.verified_at.load();
        let revision_now = zalsa.current_revision();

        if verified_at == revision_now {
            // Already verified.
            return ShallowUpdate::Verified;
        }

        self.shallow_verify_memo_cold(zalsa, database_key_index, verified_at)
    }

    #[cold]
    #[inline(never)]
    fn shallow_verify_memo_cold(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
        verified_at: Revision,
    ) -> ShallowUpdate {
        let last_changed = zalsa.last_changed_revision(self.revisions.durability);
        crate::tracing::trace!(
            "{database_key_index:?}: check_durability({database_key_index:#?}, last_changed={:?} <= verified_at={:?}) = {:?}",
            last_changed,
            verified_at,
            last_changed <= verified_at,
        );
        if last_changed <= verified_at {
            // No input of the suitable durability has changed since last verified.
            ShallowUpdate::HigherDurability
        } else {
            ShallowUpdate::No
        }
    }

    /// Validates this memo if it is a provisional memo. Returns true for:
    /// * non provisional memos
    /// * provisional memos that have been successfully marked as verified final, that is, its
    ///   cycle heads have all been finalized.
    /// * provisional memos that have been created in the same revision and iteration and are part of the same cycle.
    #[inline]
    fn validate_may_be_provisional(
        &self,
        zalsa: &Zalsa,
        zalsa_local: &ZalsaLocal,
        database_key_index: DatabaseKeyIndex,
        has_value: bool,
    ) -> bool {
        if !self.may_be_provisional() {
            return matches!(self.attempt_reuse(zalsa), MemoReuse::Ordinary);
        }

        // A missing provisional value is poison, not a participant awaiting finality.
        if !has_value {
            return false;
        }

        let cycle_heads = self.cycle_heads();
        if self.is_finality_candidate()
            && validate_provisional(zalsa, database_key_index, self, cycle_heads)
        {
            return true;
        }

        // Completed components can predate cancellation or the producer's later refusal.
        // Only unresolved approximations require current owner and cancellation permission.
        if !self.provisional_epoch_is_current(zalsa) {
            return false;
        }
        if cycle_heads.is_empty() {
            return true;
        }

        for cycle_head in cycle_heads {
            let Some(head_memo) = zalsa
                .lookup_ingredient(cycle_head.database_key_index.ingredient_index())
                .as_function()
                .and_then(|function| {
                    function.memo(zalsa, cycle_head.database_key_index.key_index())
                })
            else {
                return false;
            };

            let head_header = head_memo.header();
            if !self.provisional_head_support_matches(zalsa, head_header) {
                return false;
            }
        }

        #[cfg(feature = "detailed-trace")]
        crate::tracing::trace!(
            "{database_key_index:?}: validate_may_be_provisional(memo = {memo:#?})",
            memo = self.tracing_debug(has_value),
        );

        validate_same_iteration(zalsa, zalsa_local, database_key_index, self, cycle_heads)
    }

    pub(super) fn provisional_epoch_is_current(&self, zalsa: &Zalsa) -> bool {
        matches!(self.attempt_reuse(zalsa), MemoReuse::Ordinary)
            && self.revisions.iteration().cancellation_count()
                == zalsa.runtime().cancellation_count()
    }

    fn provisional_head_support_matches(&self, zalsa: &Zalsa, head: &Self) -> bool {
        self.same_attempt_owner(head) && matches!(head.attempt_reuse(zalsa), MemoReuse::Ordinary)
    }

    pub(super) fn provisional_head_matches(
        &self,
        zalsa: &Zalsa,
        head: &Self,
        expected_iteration: IterationStamp,
    ) -> bool {
        self.provisional_head_support_matches(zalsa, head)
            && head.verified_at.load() == self.verified_at.load()
            && head.revisions.iteration() == expected_iteration
    }
}

fn maybe_changed_after_cold_cycle(
    zalsa_local: &ZalsaLocal,
    database_key_index: DatabaseKeyIndex,
    cycle_recovery_strategy: CycleRecoveryStrategy,
) -> VerifyResult {
    match cycle_recovery_strategy {
        // SAFETY: We do not access the query stack reentrantly.
        CycleRecoveryStrategy::Panic => unsafe {
            zalsa_local.with_query_stack_unchecked(|stack| {
                panic!(
                    "dependency graph cycle when validating {database_key_index:#?}, \
                    set cycle_fn/cycle_initial to fixpoint iterate.\n\
                    Query stack:\n{stack:#?}",
                );
            })
        },
        // We flatten the dependencies of queries with cycle handling that participate in a query.
        // Verifying those queries should never result in a cycle because all function dependencies were removed.
        // That means, if we hit this path, then some query introduced a new cycle that didn't exist
        // in the previous revision. We have to consider this query changed so that we ultimately
        // insert the fixpoint initial value in `fetch_cold_cycle`.
        CycleRecoveryStrategy::FallbackImmediate | CycleRecoveryStrategy::Fixpoint => {
            crate::tracing::debug!(
                "hit cycle at {database_key_index:?} in `maybe_changed_after`,  returning changed",
            );

            VerifyResult::changed()
        }
    }
}

/// Check if this memo's cycle heads have all been finalized. If so, mark it verified final and
/// return true, if not return false.
fn validate_provisional(
    zalsa: &Zalsa,
    database_key_index: DatabaseKeyIndex,
    memo_header: &MemoHeader,
    cycle_heads: &CycleHeads,
) -> bool {
    crate::tracing::trace!("{database_key_index:?}: validate_provisional({database_key_index:?})",);

    let Some(execution_revision) = memo_header.revisions.execution_revision() else {
        return false;
    };
    if cycle_heads.is_empty() {
        return false;
    }
    for cycle_head in cycle_heads {
        let Some(function) = zalsa
            .lookup_ingredient(cycle_head.database_key_index.ingredient_index())
            .as_function()
        else {
            return false;
        };

        let head_key = cycle_head.database_key_index.key_index();
        let Some(head_memo) = function.memo(zalsa, head_key) else {
            return false;
        };
        let head_header = head_memo.header();
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::finality_head_selected(
            database_key_index,
            std::ptr::from_ref(memo_header).addr(),
            cycle_head.database_key_index,
            std::ptr::from_ref(head_header).addr(),
        );

        // Acquire acceptance before reading its stamp, and use this one selected allocation
        // for every part of the certificate. Successful later validation may advance
        // verified_at; it does not change the execution that accepted this component.
        let accepted = !head_header.may_be_provisional()
            && !head_header.has_incomplete_attempt()
            && memo_header.same_attempt_owner(head_header)
            && head_header.revisions.execution_revision() == Some(execution_revision)
            && head_header.revisions.iteration() == cycle_head.iteration.load();
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = Event::new(Kind::Verification)
                .key(database_key_index)
                .memo(Some(memo_header.transfer_test_snapshot(true)))
                .decision(accepted);
            event.phase = Some("finality.head");
            event.other_key = Some(cycle_head.database_key_index);
            event.other_memo = Some(head_memo.transfer_test_snapshot());
            transfer_trace::record(event);
        }
        // It's important to also account for the iteration for the case where:
        // thread 1: `b` -> `a` (but only in the first iteration)
        //               -> `c` -> `b`
        // thread 2: `a` -> `b`
        //
        // If we don't account for the iteration, then `a` (from iteration 0) will be finalized
        // because its cycle head `b` is now finalized, but `b` never pulled `a` in the last iteration.
        if !accepted {
            return false;
        }
    }
    // Relay the accepted root publication acquired from the finalized cycle heads above.
    memo_header
        .revisions
        .verified_final
        .store(true, Ordering::Release);
    #[cfg(all(test, not(feature = "shuttle")))]
    {
        let mut event = Event::new(Kind::Verification)
            .key(database_key_index)
            .memo(Some(memo_header.transfer_test_snapshot(true)))
            .decision(true);
        event.phase = Some("finality.published");
        transfer_trace::record(event);
    }
    true
}

/// If this is a provisional memo, validate that it was cached in the same iteration of the
/// same cycle(s) that we are still executing. If so, it is valid for reuse. This avoids
/// runaway re-execution of the same queries within a fixpoint iteration.
fn validate_same_iteration(
    zalsa: &Zalsa,
    zalsa_local: &ZalsaLocal,
    memo_database_key_index: DatabaseKeyIndex,
    memo_header: &MemoHeader,
    cycle_heads: &CycleHeads,
) -> bool {
    crate::tracing::trace!("validate_same_iteration({memo_database_key_index:?})",);

    // This is an optimization to avoid unnecessary re-execution within the same revision.
    // Don't apply it when verifying memos from past revisions. We want them to re-execute
    // to verify their cycle heads and all participating queries.
    if memo_header.verified_at.load() != zalsa.current_revision() {
        return false;
    }

    // Always return `false` for cycle initial values "unless" they are running in the same thread.
    if cycle_heads
        .iter_not_eq(memo_database_key_index)
        .next()
        .is_none()
    {
        // SAFETY: We do not access the query stack reentrantly.
        let on_stack = unsafe {
            zalsa_local.with_query_stack_unchecked(|stack| {
                stack
                    .iter()
                    .rev()
                    .any(|query| query.database_key_index == memo_database_key_index)
            })
        };

        return on_stack;
    }

    let cycle_heads_iter = TryClaimCycleHeadsIter::new(zalsa, cycle_heads);

    for cycle_head in cycle_heads_iter {
        match cycle_head {
            TryClaimHeadsResult::Cycle {
                head_iteration,
                header,
            } => {
                if !memo_header.provisional_head_matches(zalsa, header, head_iteration) {
                    return false;
                }
            }
            _ => {
                return false;
            }
        }
    }

    true
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) enum ShallowUpdate {
    /// The memo is from this revision and has already been verified
    Verified,

    /// The revision for the memo's durability hasn't changed. It can be marked as verified
    /// in this revision.
    HigherDurability,

    /// The memo requires a deep verification.
    No,
}

impl ShallowUpdate {
    pub(super) fn yes(&self) -> bool {
        matches!(
            self,
            ShallowUpdate::Verified | ShallowUpdate::HigherDurability
        )
    }
}
