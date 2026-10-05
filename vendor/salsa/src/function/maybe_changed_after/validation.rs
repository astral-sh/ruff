use super::{ShallowUpdate, ValidationProbe, VerifyResult};
#[cfg(feature = "accumulator")]
use crate::accumulator::accumulated_map::InputAccumulatedValues;
use crate::attempt_probe::MemoReuse;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{self as transfer_trace, Event, Kind};
use crate::cycle::CycleRecoveryStrategy;
use crate::database::RawDatabase;
#[cfg(test)]
use crate::function::execute::execution_run::{TraceEvent, record_validation_trace};
use crate::function::execute::participant::{Consumer, Participant, ParticipantProgress};
use crate::function::memo::{ErasedMemo, Memo, MemoHeader, MemoOutputCheck, MemoVerification};
use crate::function::sync::{ClaimGuard, ClaimResult};
use crate::function::{Configuration, IngredientImpl, Reentrancy};
use crate::runtime::Running;
use crate::zalsa::{MemoIngredientIndex, ZalsaDatabase};
use crate::zalsa_local::{OutputOrder, QueryEdgeIter, QueryEdgeKind, QueryOriginRef};
use crate::{DatabaseKeyIndex, Id, Revision};

/// The caller retains one policy operation while these states validate, reexecute, or retry.
pub(in crate::function) struct Validation<'db, C: Configuration> {
    pub(in crate::function) ingredient: &'db IngredientImpl<C>,
    pub(in crate::function) db: &'db C::DbView,
    database_key_index: DatabaseKeyIndex,
    revision: Revision,
    memo_ingredient_index: MemoIngredientIndex,
}

#[must_use]
pub(in crate::function) enum ValidationStep<'db, C: Configuration> {
    Probe(Validation<'db, C>),
    Claim(Validation<'db, C>),
    // Running owns mutex guards. Consumers must wait or release it immediately, never suspend.
    Wait(Validation<'db, C>, Running<'db>),
    Reload(Validation<'db, C>, ClaimGuard<'db>),
    Verify(Validation<'db, C>, Verification<'db>),
    Execute(ValidationExecution<'db, C>),
    Participant(Validation<'db, C>, Participant<'db, C>),
    Complete(VerifyResult),
}

impl<'db, C: Configuration> Validation<'db, C> {
    #[cfg(test)]
    pub(in crate::function) fn id(&self) -> Id {
        self.database_key_index.key_index()
    }

    pub(in crate::function) fn new(
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        id: Id,
        revision: Revision,
    ) -> Self {
        let (zalsa, zalsa_local) = db.zalsas();
        let memo_ingredient_index = ingredient.memo_ingredient_index(zalsa, id);
        zalsa.unwind_if_revision_cancelled(zalsa_local);
        Self {
            ingredient,
            db,
            database_key_index: ingredient.database_key_index(id),
            revision,
            memo_ingredient_index,
        }
    }

    #[inline]
    pub(in crate::function) fn probe(&self) -> Option<VerifyResult> {
        match self.probe_deferred() {
            ValidationProbe::Miss => None,
            ValidationProbe::Ready(result) => Some(result),
            ValidationProbe::Verify(verification) => {
                verification.event();
                Some(verification.finish())
            }
        }
    }

    #[inline]
    pub(in crate::function) fn probe_deferred(&self) -> ValidationProbe<'db> {
        self.probe_memo(self.lookup_memo())
    }

    #[inline]
    pub(in crate::function) fn lookup_memo(&self) -> Option<&'db Memo<C>> {
        let database_key_index = self.database_key_index;
        let revision = self.revision;
        let zalsa = self.db.zalsa();
        crate::tracing::debug!(
            "{database_key_index:?}: maybe_changed_after(revision = {revision:?})"
        );

        // Check if we have a verified version: this is the hot path.
        self.ingredient.get_memo_from_table_for(
            zalsa,
            database_key_index.key_index(),
            self.memo_ingredient_index,
        )
    }

    #[inline]
    pub(in crate::function) fn probe_memo(
        &self,
        memo: Option<&'db Memo<C>>,
    ) -> ValidationProbe<'db> {
        let Some(memo) = memo else {
            // No memo? Assume has changed.
            return ValidationProbe::Ready(VerifyResult::changed());
        };
        memo.header.maybe_changed_after_probe(
            self.db.zalsa(),
            self.database_key_index,
            self.revision,
            #[cfg(feature = "detailed-trace")]
            memo.value.is_some(),
        )
    }

    pub(in crate::function) fn claim(self) -> ValidationStep<'db, C> {
        match self.try_claim() {
            ClaimResult::Claimed(claim) => ValidationStep::Reload(self, claim),
            ClaimResult::Running(blocked_on) => ValidationStep::Wait(self, blocked_on),
            ClaimResult::Cycle { .. } => ValidationStep::Complete(self.cold_cycle()),
        }
    }

    pub(in crate::function) fn try_claim(&self) -> ClaimResult<'db> {
        self.ingredient.sync_table.try_claim(
            self.db.zalsa(),
            self.db.zalsa_local(),
            self.database_key_index.key_index(),
            Reentrancy::Deny,
        )
    }

    pub(in crate::function) fn cold_cycle(&self) -> VerifyResult {
        super::maybe_changed_after_cold_cycle(
            self.db.zalsa_local(),
            self.database_key_index,
            C::CYCLE_STRATEGY,
        )
    }

    pub(in crate::function) fn reload(self, claim: ClaimGuard<'db>) -> ValidationStep<'db, C> {
        // Load the current memo after claiming the query because it may have changed while
        // this query was blocked on another thread.
        let Some(memo) = self
            .ingredient
            .memo_slot(
                self.db.zalsa(),
                self.database_key_index.key_index(),
                self.memo_ingredient_index,
            )
            .get_erased()
        else {
            return ValidationStep::Complete(VerifyResult::changed());
        };
        let header = memo.header();
        if !matches!(header.attempt_reuse(self.db.zalsa()), MemoReuse::Ordinary)
            && !header.is_finality_candidate()
        {
            return ValidationStep::Complete(VerifyResult::changed());
        }

        crate::tracing::debug!(
            "{database_key_index:?}: maybe_changed_after_cold, successful claim, \
                revision = {revision:?}, old_memo = {old_memo:#?}",
            database_key_index = self.database_key_index,
            revision = self.revision,
            old_memo = header.tracing_debug(memo.has_value()),
        );
        ValidationStep::Verify(self, ClaimedMemo { claim, memo }.verify(C::CYCLE_STRATEGY))
    }

    pub(in crate::function) fn verified(
        self,
        verified: VerifiedMemo<'db>,
    ) -> ValidationStep<'db, C> {
        let VerifiedMemo { claimed, result } = verified;
        let header = claimed.memo.header();
        if let MemoValidity::Final(validity) = result {
            return ValidationStep::Complete(if header.revisions.changed_at > self.revision {
                VerifyResult::changed()
            } else {
                validity.unchanged_for(header)
            });
        }

        if let MemoValidity::Provisional(permission) = result {
            let participant = Participant::cached(
                self.ingredient,
                self.db,
                claimed.claim,
                claimed.memo.downcast::<C>(),
                Consumer::Validation,
                permission.0,
            );
            return ValidationStep::Participant(self, participant);
        }

        // If the memo is not provisional, the generic continuation can check whether it has
        // an old value and re-execute. The result may equal the old value and be backdated, in
        // which case the new memo has not logically changed.
        if header.may_be_provisional() {
            return ValidationStep::Complete(VerifyResult::changed());
        }
        let old_memo = claimed.memo.downcast::<C>();
        if old_memo.value.is_none() {
            return ValidationStep::Complete(VerifyResult::changed());
        }
        ValidationStep::Execute(ValidationExecution {
            validation: self,
            claim: claimed.claim,
            old_memo,
        })
    }

    pub(in crate::function) fn executed(
        self,
        memo: Option<&'db Memo<C>>,
    ) -> ValidationStep<'db, C> {
        let Some(memo) = memo else {
            return ValidationStep::Probe(self);
        };

        // Always assume that a provisional value has changed.
        //
        // We don't know if a provisional value has actually changed. To determine whether
        // a provisional value has changed, we need to iterate the outer cycle, which cannot
        // be done here.
        ValidationStep::Complete(
            if memo.header.revisions.changed_at > self.revision
                || memo.header.may_be_provisional()
                || !matches!(
                    memo.header.attempt_reuse(self.db.zalsa()),
                    MemoReuse::Ordinary
                )
            {
                VerifyResult::changed()
            } else {
                VerifyResult::unchanged_for_memo(&memo.header.revisions)
            },
        )
    }

    // Keep the claim-owning states and their drop paths out of the ordinary hot-call frame.
    #[inline(never)]
    pub(super) fn execute_cold(self) -> VerifyResult {
        let mut step = ValidationStep::Claim(self);
        loop {
            step = match step {
                ValidationStep::Probe(validation) => match validation.probe() {
                    Some(result) => ValidationStep::Complete(result),
                    None => ValidationStep::Claim(validation),
                },
                ValidationStep::Claim(validation) => validation.claim(),
                ValidationStep::Wait(validation, blocked_on) => {
                    let _ = blocked_on.block_on(validation.db.zalsa());
                    ValidationStep::Probe(validation)
                }
                ValidationStep::Reload(validation, claim) => validation.reload(claim),
                ValidationStep::Verify(validation, verification) => {
                    let verified = verification.execute(validation.db.into());
                    validation.verified(verified)
                }
                ValidationStep::Execute(request) => request.execute(),
                ValidationStep::Participant(validation, participant) => match participant.execute()
                {
                    ParticipantProgress::Complete(memo) => validation.executed(memo),
                    ParticipantProgress::Execute(execution) => {
                        validation.executed(execution.execute_to_completion())
                    }
                    ParticipantProgress::Pending(participant) => {
                        ValidationStep::Participant(validation, participant)
                    }
                },
                ValidationStep::Complete(result) => return result,
            };
        }
    }
}

/// Reexecution retains the original claim and the exact memo used for backdating.
pub(in crate::function) struct ValidationExecution<'db, C: Configuration> {
    pub(in crate::function) validation: Validation<'db, C>,
    pub(in crate::function) claim: ClaimGuard<'db>,
    pub(in crate::function) old_memo: &'db Memo<C>,
}

impl<'db, C: Configuration> ValidationExecution<'db, C> {
    fn execute(self) -> ValidationStep<'db, C> {
        let memo = self.validation.ingredient.execute(
            self.validation.db,
            self.claim,
            Some(self.old_memo),
            Consumer::Validation,
        );
        self.validation.executed(memo)
    }
}

/// All memo-verification continuations retain the claim, including changed results.
pub(in crate::function) struct ClaimedMemo<'db> {
    pub(in crate::function) claim: ClaimGuard<'db>,
    pub(in crate::function) memo: ErasedMemo<'db>,
}

impl<'db> ClaimedMemo<'db> {
    #[cfg(test)]
    pub(in crate::function) fn trace(&self, phase: &'static str) {
        let header = self.memo.header();
        record_validation_trace(TraceEvent::Verifier {
            phase,
            key: self.claim.database_key_index(),
            claim: self.claim.test_serial(),
            memo: std::ptr::from_ref(header).addr(),
            verified_at: header.verified_at.load(),
            current_revision: self.claim.zalsa().current_revision(),
            accumulated: {
                #[cfg(feature = "accumulator")]
                {
                    Some(header.revisions.accumulated_inputs.load().is_any())
                }
                #[cfg(not(feature = "accumulator"))]
                {
                    None
                }
            },
            owner: self.claim.zalsa_local().active_query().map(|(key, _)| key),
            depths: crate::attempt_probe::stack_depths(),
        });
    }

    /// Checks whether the value and `changed_at` are valid in the current revision, updating
    /// `verified_at` when all dependencies are unchanged. The claim remains owned while walking
    /// dependencies, which may execute queries to determine whether their outputs changed.
    pub(in crate::function) fn verify(self, strategy: CycleRecoveryStrategy) -> Verification<'db> {
        Verification {
            claimed: self,
            phase: VerificationPhase::Shallow(strategy),
        }
    }
}

pub(in crate::function) struct VerifiedMemo<'db> {
    pub(in crate::function) claimed: ClaimedMemo<'db>,
    pub(in crate::function) result: MemoValidity,
}

/// Final verification preserves the inputs' accumulator summary separately from direct values.
#[derive(Clone, Copy)]
pub(in crate::function) struct FinalValidity {
    #[cfg(feature = "accumulator")]
    inputs: InputAccumulatedValues,
}

impl FinalValidity {
    fn from_header(_header: &MemoHeader) -> Self {
        Self {
            #[cfg(feature = "accumulator")]
            inputs: _header.revisions.accumulated_inputs.load(),
        }
    }

    fn unchanged_for(self, _header: &MemoHeader) -> VerifyResult {
        VerifyResult::unchanged_with_accumulated(
            #[cfg(feature = "accumulator")]
            if _header.revisions.accumulated().is_some() {
                InputAccumulatedValues::Any
            } else {
                self.inputs
            },
        )
    }
}

/// Compatibility of an unresolved approximation requires owned participant retirement.
pub(in crate::function) struct ProvisionalValidity(
    pub(in crate::function) crate::cycle::CycleHeads,
);

pub(in crate::function) enum MemoValidity {
    Final(FinalValidity),
    Provisional(ProvisionalValidity),
    Changed,
}

impl MemoValidity {
    #[cfg(test)]
    pub(in crate::function) fn is_unchanged(&self) -> bool {
        !matches!(self, Self::Changed)
    }
}

/// The claim and exact memo stay owned while local phases and dependencies advance in place.
#[must_use]
pub(in crate::function) struct Verification<'db> {
    claimed: ClaimedMemo<'db>,
    phase: VerificationPhase<'db>,
}

enum VerificationPhase<'db> {
    Shallow(CycleRecoveryStrategy),
    Deep(CycleRecoveryStrategy),
    Edges(VerifyEdges<'db>),
    Event {
        revision: Revision,
        result: FinalValidity,
        origin: VerificationOrigin,
    },
    Complete(MemoValidity),
}

#[derive(Clone, Copy)]
enum VerificationOrigin {
    Shallow,
    Deep,
}

impl<'db> VerificationPhase<'db> {
    fn complete(&mut self, _claimed: &ClaimedMemo<'_>, result: MemoValidity) {
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            Event::new(Kind::Verification)
                .key(_claimed.claim.database_key_index())
                .serial(_claimed.claim.test_serial())
                .memo(Some(_claimed.memo.transfer_test_snapshot()))
                .decision(result.is_unchanged()),
        );
        #[cfg(test)]
        _claimed.trace("completed");
        *self = Self::Complete(result);
    }

    fn advance_shallow(&mut self, claimed: &ClaimedMemo<'db>, strategy: CycleRecoveryStrategy) {
        let header = claimed.memo.header();
        let zalsa = claimed.claim.zalsa();
        let database_key_index = claimed.claim.database_key_index();
        if !matches!(header.attempt_reuse(zalsa), MemoReuse::Ordinary)
            && !header.is_finality_candidate()
        {
            return self.complete(claimed, MemoValidity::Changed);
        }
        let can_shallow_update = header.shallow_verify_memo(
            zalsa,
            database_key_index,
            #[cfg(feature = "detailed-trace")]
            claimed.memo.has_value(),
        );
        if can_shallow_update.yes()
            && header.validate_may_be_provisional(
                zalsa,
                claimed.claim.zalsa_local(),
                database_key_index,
                claimed.memo.has_value(),
            )
        {
            if can_shallow_update == ShallowUpdate::HigherDurability {
                *self = Self::Event {
                    revision: zalsa.current_revision(),
                    result: FinalValidity::from_header(header),
                    origin: VerificationOrigin::Shallow,
                };
            } else {
                #[cfg(test)]
                claimed.trace("shallow.updated");
                self.complete(
                    claimed,
                    if header.may_be_provisional() {
                        MemoValidity::Provisional(ProvisionalValidity(
                            header.revisions.cycle_heads().clone(),
                        ))
                    } else {
                        MemoValidity::Final(FinalValidity::from_header(header))
                    },
                );
            }
        } else {
            *self = VerificationPhase::Deep(strategy);
        }
    }

    fn advance_deep(&mut self, claimed: &ClaimedMemo<'db>, strategy: CycleRecoveryStrategy) {
        let header = claimed.memo.header();
        let zalsa = claimed.claim.zalsa();

        // An incomplete result may have no retained dependency that exposes the refusal,
        // especially after cycle dependencies have been flattened.
        if !matches!(header.attempt_reuse(zalsa), MemoReuse::Ordinary) {
            return self.complete(claimed, MemoValidity::Changed);
        }
        // Inherited outputs retain ownership, but their positions do not record the inputs
        // that caused their creation. After shallow verification fails, reexecute before
        // validating any edge: marking these outputs early could lock entities that the
        // new execution must delete. Delaying them could instead leave a later input
        // unable to read an earlier specified value.
        if header.revisions.output_order() == OutputOrder::Inherited {
            return self.complete(claimed, MemoValidity::Changed);
        }
        match header.origin() {
            QueryOriginRef::Derived(edges) => {
                #[cfg(feature = "detailed-trace")]
                crate::tracing::debug!(
                    "{database_key_index:?}: deep_verify_memo(old_memo = {old_memo:#?})",
                    database_key_index = claimed.claim.database_key_index(),
                    old_memo = header.tracing_debug(claimed.memo.has_value()),
                );

                // If the value is from the same revision but is still provisional, consider it changed
                // because we're now in a new iteration.
                if header.may_be_provisional() {
                    return self.complete(claimed, MemoValidity::Changed);
                }

                // If the old memo participate in a cycle, but the query doesn't have cycle handling,
                // always return changed. The reasoning here is:
                //
                // * cycle heads flatten their dependecies. Therefore, no query with cycle handling
                //   participating in the same cycle should ever call `maybe_changed_after` on any other query.
                //   (we don't get here).
                // * the query can't be reached from any other query without cycle handling because,
                //   executing it would immediately panic because of the cycle.
                // * The only other place where we can reach this code is from `fetch`, this is when
                //   the outer cycle is being re-executed. Given that the cycle re-executes, this
                //   query must always be considered changed.
                //
                // For queries with cycle handling, verify the flattened
                // dependencies of the cycle head instead.
                if strategy == CycleRecoveryStrategy::Panic && header.was_cycle_participant() {
                    return self.complete(claimed, MemoValidity::Changed);
                }
                let old_verified_at = header.verified_at.load();
                *self = VerificationPhase::Edges(VerifyEdges {
                    edges: edges.iter(),
                    old_verified_at,
                    #[cfg(feature = "accumulator")]
                    inputs: InputAccumulatedValues::Empty,
                    phase: EdgePhase::Next,
                });
            }
            QueryOriginRef::Assigned(_) => {
                // If the value was assigned by another query,
                // and that query were up-to-date,
                // then we would have updated the `verified_at` field already.
                // So the fact that we are here means that it was not specified
                // during this revision or is otherwise stale.
                //
                // Example of how this can happen:
                //
                // Conditionally specified queries
                // where the value is specified
                // in rev 1 but not in rev 2.
                self.complete(claimed, MemoValidity::Changed)
            }
            QueryOriginRef::DerivedUntracked(_) => {
                // Untracked inputs? Have to assume that it changed.
                self.complete(claimed, MemoValidity::Changed)
            }
        }
    }

    fn commit(&mut self, claimed: &ClaimedMemo<'db>, result: VerifyResult) {
        let header = claimed.memo.header();
        let zalsa = claimed.claim.zalsa();
        if !matches!(header.attempt_reuse(zalsa), MemoReuse::Ordinary) {
            #[cfg(test)]
            claimed.trace("commit.incomplete");
            return self.complete(claimed, MemoValidity::Changed);
        }
        if let VerifyResult::Unchanged {
            #[cfg(feature = "accumulator")]
            accumulated,
        } = result
        {
            *self = Self::Event {
                revision: zalsa.current_revision(),
                result: FinalValidity {
                    #[cfg(feature = "accumulator")]
                    inputs: accumulated,
                },
                origin: VerificationOrigin::Deep,
            };
            return;
        }
        #[cfg(test)]
        claimed.trace("commit.changed");
        self.complete(claimed, MemoValidity::Changed)
    }
}

impl<'db> Verification<'db> {
    pub(in crate::function) fn output_check_work(&self, check: MemoOutputCheck) -> usize {
        self.claimed.memo.header().output_check_work(check)
    }

    pub(in crate::function) fn outputs_are_empty(&self) -> bool {
        self.claimed.memo.header().outputs_are_empty()
    }

    pub(in crate::function) fn controlled_outputs_are_empty(&self) -> bool {
        self.claimed.memo.header().controlled_outputs_are_empty()
    }

    pub(in crate::function) fn pending_event(
        &mut self,
    ) -> Option<PendingVerificationEvent<'_, 'db>> {
        match self.phase {
            VerificationPhase::Event {
                revision,
                result,
                origin,
            } => Some(PendingVerificationEvent::new(
                &self.claimed,
                &mut self.phase,
                revision,
                result,
                origin,
            )),
            _ => None,
        }
    }

    pub(in crate::function) fn into_claim(self) -> ClaimGuard<'db> {
        self.claimed.claim
    }

    /// An early completion attempt returns all ownership to its cleanup guard.
    pub(in crate::function) fn complete(self) -> Result<VerifiedMemo<'db>, Self> {
        match self.phase {
            VerificationPhase::Complete(result) => Ok(VerifiedMemo {
                claimed: self.claimed,
                result,
            }),
            _ => Err(self),
        }
    }

    #[cfg(test)]
    pub(in crate::function) fn owner(&self) -> DatabaseKeyIndex {
        self.claimed.claim.database_key_index()
    }

    #[cfg(test)]
    pub(in crate::function) fn phase_name(&self) -> &'static str {
        match self.phase {
            VerificationPhase::Shallow(_) => "shallow",
            VerificationPhase::Deep(_) => "deep",
            VerificationPhase::Edges(ref cursor) => match cursor.phase {
                EdgePhase::Next => "edge",
                EdgePhase::Waiting(_) => "dependency.demand",
                EdgePhase::Finish => "finish",
                EdgePhase::Commit(_) => "commit",
            },
            VerificationPhase::Event { .. } => "verification.event",
            VerificationPhase::Complete(_) => "verification.complete",
        }
    }

    pub(in crate::function) fn action(&mut self) -> VerificationAction<'_, 'db> {
        let claimed = &self.claimed;
        let work = match self.phase {
            VerificationPhase::Event {
                revision,
                result,
                origin,
            } => {
                return VerificationAction::Event(PendingVerificationEvent::new(
                    claimed,
                    &mut self.phase,
                    revision,
                    result,
                    origin,
                ));
            }
            VerificationPhase::Shallow(strategy) => VerificationWorkKind::Shallow {
                phase: &mut self.phase,
                strategy,
            },
            VerificationPhase::Deep(strategy) => VerificationWorkKind::Deep {
                phase: &mut self.phase,
                strategy,
            },
            VerificationPhase::Edges(VerifyEdges {
                phase: EdgePhase::Commit(result),
                ..
            }) => VerificationWorkKind::Commit {
                phase: &mut self.phase,
                result,
            },
            VerificationPhase::Edges(
                ref mut cursor @ VerifyEdges {
                    phase: EdgePhase::Next,
                    ..
                },
            ) => VerificationWorkKind::Edge(cursor),
            VerificationPhase::Edges(
                ref mut cursor @ VerifyEdges {
                    phase: EdgePhase::Finish,
                    ..
                },
            ) => VerificationWorkKind::Finish(cursor),
            VerificationPhase::Edges(
                ref mut cursor @ VerifyEdges {
                    phase: EdgePhase::Waiting(request),
                    ..
                },
            ) => {
                return VerificationAction::Dependency {
                    request,
                    reply: PendingInput {
                        cursor,
                        #[cfg(test)]
                        owner: claimed.claim.database_key_index(),
                        #[cfg(test)]
                        key: request.key,
                    },
                };
            }
            VerificationPhase::Complete(_) => return VerificationAction::Complete,
        };
        VerificationAction::Work(VerificationWork { claimed, work })
    }

    pub(in crate::function) fn execute(mut self, db: RawDatabase<'db>) -> VerifiedMemo<'db> {
        let zalsa = self.claimed.claim.zalsa();
        loop {
            self = match self.complete() {
                Ok(verified) => return verified,
                Err(verification) => verification,
            };
            match self.action() {
                VerificationAction::Work(work) => work.advance(),
                VerificationAction::Event(event) => {
                    event.event();
                    event.finish();
                }
                VerificationAction::Dependency { request, reply } => {
                    let result = request
                        .key
                        .maybe_changed_after(db, zalsa, request.changed_after);
                    reply.resume(result);
                }
                VerificationAction::Complete => {}
            }
        }
    }
}

#[must_use]
pub(in crate::function) enum VerificationAction<'a, 'db> {
    Work(VerificationWork<'a, 'db>),
    Event(PendingVerificationEvent<'a, 'db>),
    Dependency {
        request: VerifyDependency,
        reply: PendingInput<'a, 'db>,
    },
    Complete,
}

/// The verifier keeps its claim while the event is borrowed, accepted, and then stamped.
pub(in crate::function) struct PendingVerificationEvent<'a, 'db> {
    claimed: &'a ClaimedMemo<'db>,
    phase: &'a mut VerificationPhase<'db>,
    verification: MemoVerification<'db>,
    result: FinalValidity,
    origin: VerificationOrigin,
}

impl<'a, 'db> PendingVerificationEvent<'a, 'db> {
    fn new(
        claimed: &'a ClaimedMemo<'db>,
        phase: &'a mut VerificationPhase<'db>,
        revision: Revision,
        result: FinalValidity,
        origin: VerificationOrigin,
    ) -> Self {
        Self {
            claimed,
            phase,
            verification: MemoVerification::new(
                claimed.memo.header(),
                claimed.claim.zalsa(),
                claimed.claim.database_key_index(),
                revision,
            ),
            result,
            origin,
        }
    }

    pub(in crate::function) fn event(&self) {
        self.verification.event();
    }

    pub(in crate::function) fn finish(self) {
        match self.origin {
            VerificationOrigin::Shallow => {
                self.verification.publish_shallow();
                #[cfg(test)]
                self.claimed.trace("shallow.updated");
            }
            VerificationOrigin::Deep => {
                self.verification.publish();
                #[cfg(test)]
                self.claimed.trace("commit.published");
            }
        }
        self.phase
            .complete(self.claimed, MemoValidity::Final(self.result));
    }
}

/// A work handle borrows only a phase that can advance without a dependency response.
pub(in crate::function) struct VerificationWork<'a, 'db> {
    claimed: &'a ClaimedMemo<'db>,
    work: VerificationWorkKind<'a, 'db>,
}

enum VerificationWorkKind<'a, 'db> {
    Shallow {
        phase: &'a mut VerificationPhase<'db>,
        strategy: CycleRecoveryStrategy,
    },
    Deep {
        phase: &'a mut VerificationPhase<'db>,
        strategy: CycleRecoveryStrategy,
    },
    Edge(&'a mut VerifyEdges<'db>),
    Finish(&'a mut VerifyEdges<'db>),
    Commit {
        phase: &'a mut VerificationPhase<'db>,
        result: VerifyResult,
    },
}

impl VerificationWork<'_, '_> {
    pub(in crate::function) fn provisional_work(&self) -> Option<(usize, usize)> {
        if !matches!(self.work, VerificationWorkKind::Shallow { .. })
            || !self.claimed.memo.header().may_be_provisional()
        {
            return Some((0, 0));
        }
        let entries = self
            .claimed
            .memo
            .header()
            .revisions
            .cycle_heads()
            .storage_len();
        Some((
            entries.checked_mul(4)?.checked_add(1)?,
            entries
                .checked_mul(size_of::<crate::cycle::CycleHead>())?
                .checked_add(2 * size_of::<usize>())?,
        ))
    }

    #[cfg(test)]
    pub(in crate::function) fn phase_name(&self) -> &'static str {
        match self.work {
            VerificationWorkKind::Shallow { .. } => "shallow",
            VerificationWorkKind::Deep { .. } => "deep",
            VerificationWorkKind::Edge(_) => "edge",
            VerificationWorkKind::Finish(_) => "finish",
            VerificationWorkKind::Commit { .. } => "commit",
        }
    }

    /// Each local transition can be admitted before it inspects or updates memo metadata.
    #[inline]
    pub(in crate::function) fn advance(self) {
        #[cfg(test)]
        self.claimed.trace(self.phase_name());
        let claimed = self.claimed;
        match self.work {
            VerificationWorkKind::Shallow { phase, strategy } => {
                phase.advance_shallow(claimed, strategy)
            }
            VerificationWorkKind::Deep { phase, strategy } => phase.advance_deep(claimed, strategy),
            VerificationWorkKind::Edge(cursor) => cursor.advance_edge(claimed),
            VerificationWorkKind::Finish(cursor) => cursor.finish(claimed),
            VerificationWorkKind::Commit { phase, result } => phase.commit(claimed, result),
        }
    }
}

struct VerifyEdges<'db> {
    edges: QueryEdgeIter<'db>,
    old_verified_at: Revision,
    #[cfg(feature = "accumulator")]
    inputs: InputAccumulatedValues,
    phase: EdgePhase,
}

impl VerifyEdges<'_> {
    #[inline]
    fn advance_edge(&mut self, claimed: &ClaimedMemo<'_>) {
        // Fully tracked inputs? Iterate over the inputs and check them, one by one.
        //
        // NB: It's important here that we are iterating the inputs in the order that
        // they executed. It's possible that if the value of some input I0 is no longer
        // valid, then some later input I1 might never have executed at all, so verifying
        // it is still up to date is meaningless.
        let Some(edge) = self.edges.next() else {
            self.phase = EdgePhase::Finish;
            return;
        };
        match edge.kind() {
            QueryEdgeKind::Input => {
                #[cfg(test)]
                record_validation_trace(TraceEvent::Request {
                    owner: claimed.claim.database_key_index(),
                    key: edge.key(),
                    changed_after: self.old_verified_at,
                });
                self.phase = EdgePhase::Waiting(VerifyDependency {
                    key: edge.key(),
                    changed_after: self.old_verified_at,
                });
            }
            QueryEdgeKind::Output => {
                // Subtle: Mark outputs as validated now, even though we may
                // later find an input that requires us to re-execute the function.
                // Even if it re-execute, the function will wind up writing the same value,
                // since all prior inputs were green. It's important to do this during
                // this loop, because it's possible that one of our input queries will
                // re-execute and may read one of our earlier outputs
                // (e.g., in a scenario where we do something like
                // `e = Entity::new(..); query(e);` and `query` reads a field of `e`).
                //
                // NB. Accumulators are also outputs, but the above logic doesn't
                // quite apply to them. Since multiple values are pushed, the first value
                // may be unchanged, but later values could be different.
                // In that case, however, the data accumulated
                // by this function cannot be read until this function is marked green,
                // so even if we mark them as valid here, the function will re-execute
                // and overwrite the contents.
                edge.key().mark_validated_output(
                    claimed.claim.zalsa(),
                    claimed.claim.database_key_index(),
                );
                #[cfg(test)]
                record_validation_trace(TraceEvent::Output {
                    owner: claimed.claim.database_key_index(),
                    key: edge.key(),
                });
            }
        }
    }

    #[inline]
    fn finish(&mut self, _claimed: &ClaimedMemo<'_>) {
        #[cfg(feature = "accumulator")]
        let header = _claimed.memo.header();
        let result = VerifyResult::unchanged_with_accumulated(
            #[cfg(feature = "accumulator")]
            self.inputs,
        );

        // This value is only read once the memo is verified. It's therefore safe
        // to write a non-final value here.
        #[cfg(feature = "accumulator")]
        header.revisions.accumulated_inputs.store(self.inputs);
        #[cfg(all(test, feature = "accumulator"))]
        _claimed.trace("accumulated.published");

        self.phase = EdgePhase::Commit(result);
    }
}

enum EdgePhase {
    Next,
    Waiting(VerifyDependency),
    Finish,
    Commit(VerifyResult),
}

#[derive(Clone, Copy)]
pub(in crate::function) struct VerifyDependency {
    pub(in crate::function) key: DatabaseKeyIndex,
    pub(in crate::function) changed_after: Revision,
}

/// Dropping this handle leaves the cursor waiting; only a real reply resumes its traversal.
pub(in crate::function) struct PendingInput<'a, 'db> {
    cursor: &'a mut VerifyEdges<'db>,
    #[cfg(test)]
    owner: DatabaseKeyIndex,
    #[cfg(test)]
    key: DatabaseKeyIndex,
}

impl PendingInput<'_, '_> {
    #[cfg(test)]
    pub(in crate::function) fn owner(&self) -> DatabaseKeyIndex {
        self.owner
    }

    pub(in crate::function) fn resume(self, result: VerifyResult) {
        #[cfg(test)]
        record_validation_trace(TraceEvent::Reply {
            owner: self.owner,
            key: self.key,
            result,
        });
        match result {
            VerifyResult::Changed => self.cursor.phase = EdgePhase::Commit(VerifyResult::changed()),
            #[cfg(feature = "accumulator")]
            VerifyResult::Unchanged { accumulated } => {
                self.cursor.inputs |= accumulated;
                self.cursor.phase = EdgePhase::Next;
            }
            #[cfg(not(feature = "accumulator"))]
            VerifyResult::Unchanged { .. } => self.cursor.phase = EdgePhase::Next,
        }
    }
}
