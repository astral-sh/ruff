use crate::attempt_probe::MemoReuse;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{self as transfer_trace, Event, Kind};
use crate::cycle::{CycleRecoveryStrategy, IterationStamp};
use crate::function::delete::PreparedRetirement;
use crate::function::execute::participant::{Consumer, Participant, ParticipantProgress};
use crate::function::fetch::FetchProbe;
use crate::function::maybe_changed_after::validation::{
    ClaimedMemo, MemoValidity, Verification, VerifiedMemo,
};
use crate::function::memo::{Memo, PreparedMemo, SelectedMemo};
use crate::function::sync::{ClaimGuard, ClaimResult};
use crate::function::{Configuration, IngredientImpl, Reentrancy};
use crate::runtime::Running;
use crate::table::memo::PreparedMemoSlot;
use crate::zalsa::{MemoIngredientIndex, Zalsa};
use crate::zalsa_local::{QueryRevisions, ZalsaLocal};
use crate::{Cancelled, Id};

/// Selects a value-bearing memo while the caller retains the original policy operation.
/// Fetch records that selection as a read; internal refresh consumers do not.
pub(in crate::function) struct Refresh<'db, C: Configuration> {
    pub(in crate::function) ingredient: &'db IngredientImpl<C>,
    pub(in crate::function) db: &'db C::DbView,
    pub(in crate::function) id: Id,
    zalsa: &'db Zalsa,
    zalsa_local: &'db ZalsaLocal,
    memo_ingredient_index: MemoIngredientIndex,
    consumer: Consumer,
}

#[must_use]
pub(in crate::function) enum SelectionStep<'db, C: Configuration> {
    Probe(Refresh<'db, C>),
    Claim(Refresh<'db, C>),
    // Running owns mutex guards. Consumers must wait or release it immediately, never suspend.
    Wait(Refresh<'db, C>, Running<'db>),
    Reload(Refresh<'db, C>, ClaimGuard<'db>),
    Verify(Refresh<'db, C>, Verification<'db>),
    Execute(FetchExecution<'db, C>),
    Participant(Refresh<'db, C>, Participant<'db, C>),
    CycleProbe(Refresh<'db, C>),
    ColdInitial(ColdCycleInitial<'db, C>),
    ColdInitialReady(ColdCycleReady<'db, C>),
    Selected(SelectedMemo<'db, C>),
}

pub(in crate::function) enum ColdCycleDecision<'db, C: Configuration> {
    Selected(SelectedMemo<'db, C>),
    Initial(QueryRevisions),
}

impl<'db, C: Configuration> Refresh<'db, C> {
    pub(in crate::function) fn new(
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Self {
        Self {
            ingredient,
            db,
            id,
            zalsa,
            zalsa_local,
            memo_ingredient_index,
            consumer: Consumer::capture(zalsa_local),
        }
    }

    pub(in crate::function) fn requirement(&self) -> Consumer {
        self.consumer.for_child(self.zalsa_local)
    }

    pub(in crate::function) fn final_metadata(mut self) -> Self {
        self.consumer = Consumer::FinalMetadata;
        self
    }

    #[inline]
    pub(in crate::function) fn probe(&self) -> Option<SelectedMemo<'db, C>> {
        self.ingredient
            .fetch_hot(self.zalsa, self.id, self.memo_ingredient_index)
    }

    #[inline]
    pub(in crate::function) fn probe_deferred(&self) -> FetchProbe<'db, C> {
        self.ingredient
            .fetch_probe(self.zalsa, self.id, self.memo_ingredient_index)
    }

    pub(in crate::function) fn claim(self) -> SelectionStep<'db, C> {
        // Try to claim this query: if someone else has claimed it already, go back and start again.
        match self.ingredient.sync_table.try_claim(
            self.zalsa,
            self.zalsa_local,
            self.id,
            Reentrancy::Allow,
        ) {
            ClaimResult::Claimed(claim) => SelectionStep::Reload(self, claim),
            ClaimResult::Running(blocked_on) => SelectionStep::Wait(self, blocked_on),
            ClaimResult::Cycle { .. } => SelectionStep::CycleProbe(self),
        }
    }

    pub(in crate::function) fn reload(self, claim: ClaimGuard<'db>) -> SelectionStep<'db, C> {
        // Now that we've claimed the item, check again to see if there's a "hot" value.
        let old_memo = self
            .ingredient
            .memo_slot(self.zalsa, self.id, self.memo_ingredient_index)
            .get_erased();

        if let Some(old_memo) = old_memo {
            let typed_memo = old_memo.downcast::<C>();
            if typed_memo.value.is_some()
                && typed_memo.header.attempt_reuse(self.zalsa) == MemoReuse::Incomplete
                && !typed_memo.header.is_finality_candidate()
            {
                return selected(typed_memo);
            }
            if typed_memo.value.is_some() {
                return SelectionStep::Verify(
                    self,
                    ClaimedMemo {
                        claim,
                        memo: old_memo,
                    }
                    .verify(C::CYCLE_STRATEGY),
                );
            }
        }

        SelectionStep::Execute(FetchExecution {
            refresh: self,
            claim,
            old_memo: old_memo.map(|memo| memo.downcast::<C>()),
        })
    }

    pub(in crate::function) fn verified(
        self,
        verified: VerifiedMemo<'db>,
    ) -> SelectionStep<'db, C> {
        let old_memo = verified.claimed.memo.downcast::<C>();
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            Event::new(Kind::Verified)
                .key(self.ingredient.database_key_index(self.id))
                .serial(verified.claimed.claim.test_serial())
                .memo(Some(old_memo.transfer_test_snapshot()))
                .decision(verified.result.is_unchanged()),
        );
        if matches!(&verified.result, MemoValidity::Final(_))
            || old_memo.header.attempt_reuse(self.zalsa) == MemoReuse::Incomplete
        {
            // SAFETY: the final memo or this owner's incomplete result remains in the memo map.
            return selected(unsafe { self.ingredient.extend_memo_lifetime(old_memo) });
        }
        if let MemoValidity::Provisional(permission) = verified.result {
            let participant = Participant::cached(
                self.ingredient,
                self.db,
                verified.claimed.claim,
                old_memo,
                self.requirement(),
                permission.0,
            );
            return SelectionStep::Participant(self, participant);
        }
        SelectionStep::Execute(FetchExecution {
            refresh: self,
            claim: verified.claimed.claim,
            old_memo: Some(old_memo),
        })
    }

    pub(in crate::function) fn executed(self, memo: Option<&'db Memo<C>>) -> SelectionStep<'db, C> {
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            Event::new(Kind::Executed)
                .key(self.ingredient.database_key_index(self.id))
                .memo(memo.map(Memo::transfer_test_snapshot))
                .decision(memo.is_some()),
        );
        match memo {
            Some(memo) => selected(memo),
            None => SelectionStep::Probe(self),
        }
    }

    #[cold]
    #[inline(never)]
    pub(in crate::function) fn fetch_cold_cycle(self) -> SelectionStep<'db, C> {
        let decision = self.probe_cold_cycle();
        self.resume_cold_cycle(decision)
    }

    pub(in crate::function) fn resume_cold_cycle(
        self,
        decision: ColdCycleDecision<'db, C>,
    ) -> SelectionStep<'db, C> {
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let key = self.ingredient.database_key_index(self.id);
            let event = match &decision {
                ColdCycleDecision::Selected(memo) => Event::new(Kind::ColdSelected)
                    .key(key)
                    .memo(Some(memo.memo().transfer_test_snapshot())),
                ColdCycleDecision::Initial(revisions) => {
                    let mut event = Event::new(Kind::ColdInitial).key(key);
                    event.iteration = Some(revisions.iteration());
                    event.support = revisions
                        .attempt_support()
                        .map(transfer_trace::support_snapshot);
                    event
                }
            };
            transfer_trace::record(event);
        }
        match decision {
            ColdCycleDecision::Selected(memo) => SelectionStep::Selected(memo),
            ColdCycleDecision::Initial(revisions) => SelectionStep::ColdInitial(ColdCycleInitial {
                refresh: self,
                revisions,
            }),
        }
    }

    #[cold]
    #[inline(never)]
    pub(in crate::function) fn probe_cold_cycle(&self) -> ColdCycleDecision<'db, C> {
        let database_key_index = self.ingredient.database_key_index(self.id);
        let zalsa = self.zalsa;
        // no provisional value; create/insert/return initial provisional value
        match C::CYCLE_STRATEGY {
            // SAFETY: We do not access the query stack reentrantly.
            CycleRecoveryStrategy::Panic => unsafe {
                self.zalsa_local.with_query_stack_unchecked(|stack| {
                    panic!(
                        "dependency graph cycle when querying {database_key_index:#?}, \
                    set cycle_fn/cycle_initial to fixpoint iterate.\n\
                    Query stack:\n{stack:#?}",
                    );
                })
            },
            CycleRecoveryStrategy::Fixpoint | CycleRecoveryStrategy::FallbackImmediate => {
                let cancellation_count = zalsa.runtime().cancellation_count();
                // check if there's a provisional value for this query
                // Note we don't `validate_may_be_provisional` the memo here as we want to reuse an
                // existing provisional memo if it exists
                let memo_guard = self.ingredient.get_memo_from_table_for(
                    zalsa,
                    self.id,
                    self.memo_ingredient_index,
                );
                if let Some(memo) = &memo_guard {
                    let revisions = &memo.header.revisions;
                    // Don't replace a poisoned memo from this execution with a new initial value.
                    if memo.value.is_none()
                        && memo.header.may_be_provisional()
                        && memo.header.verified_at.load() == zalsa.current_revision()
                        && revisions.iteration().cancellation_count() == cancellation_count
                    {
                        Cancelled::PropagatedPanic.throw();
                    }

                    if memo.value.is_some()
                        && memo.header.attempt_reuse(zalsa) == MemoReuse::Incomplete
                    {
                        return ColdCycleDecision::Selected(selected_memo(memo));
                    }

                    self.consumer.require_seed_recipient(self.zalsa_local);

                    // Ideally, we'd use the last provisional memo even if it wasn't a cycle head in the last iteration
                    // but that would require inserting itself as a cycle head, which either requires clone
                    // on the value OR a concurrent `Vec` for cycle heads.
                    if memo.header.verified_at.load() == zalsa.current_revision()
                        && memo.header.can_seed_attempt(zalsa)
                        && memo.value.is_some()
                        && revisions.iteration().cancellation_count() == cancellation_count
                        && revisions.cycle_heads().contains(&database_key_index)
                    {
                        revisions
                            .cycle_heads()
                            .remove_all_except(database_key_index);

                        crate::tracing::debug!(
                            "hit cycle at {database_key_index:#?}, \
                                returning last provisional value: {:#?}",
                            revisions
                        );

                        // SAFETY: memo is present in memo_map.
                        return ColdCycleDecision::Selected(selected_memo(unsafe {
                            self.ingredient.extend_memo_lifetime(memo)
                        }));
                    }
                }

                crate::tracing::debug!(
                    "hit cycle at {database_key_index:#?}, \
                    inserting and returning fixpoint initial value"
                );

                let iteration = memo_guard
                    .and_then(|old_memo| {
                        let revisions = &old_memo.header.revisions;
                        if old_memo.header.verified_at.load() == zalsa.current_revision()
                            && old_memo.header.can_seed_attempt(zalsa)
                            && old_memo.value.is_some()
                            && revisions.iteration().cancellation_count() == cancellation_count
                        {
                            Some(revisions.iteration())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| IterationStamp::initial(cancellation_count));
                self.consumer.require_seed_recipient(self.zalsa_local);
                let revisions = QueryRevisions::fixpoint_initial(zalsa, database_key_index, iteration);

                ColdCycleDecision::Initial(revisions)
            }
        }
    }

    // Keep claim-owning states, callback results and their drop paths out of the hot-call frame.
    #[inline(never)]
    pub(super) fn execute_cold(
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
        metadata: bool,
    ) -> SelectedMemo<'db, C> {
        let refresh = Self::new(
            ingredient,
            db,
            zalsa,
            zalsa_local,
            id,
            memo_ingredient_index,
        );
        let refresh = if metadata {
            refresh.final_metadata()
        } else {
            refresh
        };
        let mut step = SelectionStep::Claim(refresh);
        loop {
            step = match step {
                SelectionStep::Probe(refresh) => match refresh.probe() {
                    Some(selected) => SelectionStep::Selected(selected),
                    None => SelectionStep::Claim(refresh),
                },
                SelectionStep::Claim(refresh) => refresh.claim(),
                SelectionStep::Wait(refresh, blocked_on) => {
                    let _ = blocked_on.block_on(refresh.zalsa);
                    SelectionStep::Probe(refresh)
                }
                SelectionStep::Reload(refresh, claim) => refresh.reload(claim),
                SelectionStep::Verify(refresh, verification) => {
                    let verified = verification.execute(refresh.db.into());
                    refresh.verified(verified)
                }
                SelectionStep::Execute(request) => request.execute(),
                SelectionStep::Participant(refresh, participant) => match participant.execute() {
                    ParticipantProgress::Complete(memo) => refresh.executed(memo),
                    ParticipantProgress::Execute(execution) => {
                        let memo = execution.execute_to_completion();
                        refresh.executed(memo)
                    }
                    ParticipantProgress::Pending(participant) => {
                        SelectionStep::Participant(refresh, participant)
                    }
                },
                SelectionStep::CycleProbe(refresh) => refresh.fetch_cold_cycle(),
                SelectionStep::ColdInitial(request) => request.execute(),
                SelectionStep::ColdInitialReady(request) => request.insert(),
                SelectionStep::Selected(memo) => return memo,
            };
        }
    }
}

fn selected<C: Configuration>(memo: &Memo<C>) -> SelectionStep<'_, C> {
    SelectionStep::Selected(selected_memo(memo))
}

fn selected_memo<C: Configuration>(memo: &Memo<C>) -> SelectedMemo<'_, C> {
    // Selection reaches here only after a value check or completed insertion/execution.
    // A missing value violates that contract; it is not a claim-transfer retry.
    SelectedMemo::new(memo).expect("a refreshed memo must contain a value")
}

/// Selection retains the original claim and old memo through validation-triggered execution.
pub(in crate::function) struct FetchExecution<'db, C: Configuration> {
    pub(in crate::function) refresh: Refresh<'db, C>,
    pub(in crate::function) claim: ClaimGuard<'db>,
    pub(in crate::function) old_memo: Option<&'db Memo<C>>,
}

impl<'db, C: Configuration> FetchExecution<'db, C> {
    fn execute(self) -> SelectionStep<'db, C> {
        let memo = self.refresh.ingredient.execute(
            self.refresh.db,
            self.claim,
            self.old_memo,
            self.refresh.requirement(),
        );
        self.refresh.executed(memo)
    }
}

/// A reentered target owns no new query frame or claim. These revisions are captured before
/// the initial callback, whose reads belong to the already-current caller's frame.
pub(in crate::function) struct ColdCycleInitial<'db, C: Configuration> {
    pub(in crate::function) refresh: Refresh<'db, C>,
    revisions: QueryRevisions,
}

impl<'db, C: Configuration> ColdCycleInitial<'db, C> {
    pub(in crate::function) fn returned(self, value: C::Output<'db>) -> ColdCycleReady<'db, C> {
        ColdCycleReady {
            value,
            request: self,
        }
    }

    fn execute(self) -> SelectionStep<'db, C> {
        let initial_value = C::cycle_initial(
            self.refresh.db,
            self.refresh.id,
            C::id_to_input(self.refresh.zalsa, self.refresh.id),
        );
        SelectionStep::ColdInitialReady(self.returned(initial_value))
    }
}

/// The callback's actual output stays owned until insertion is admitted. Dropping it never
/// removes the caller's frame or releases the reentered target's claim.
pub(in crate::function) struct ColdCycleReady<'db, C: Configuration> {
    pub(in crate::function) value: C::Output<'db>,
    request: ColdCycleInitial<'db, C>,
}

impl<'db, C: Configuration> ColdCycleReady<'db, C> {
    pub(in crate::function) fn storage_bytes(&self) -> Option<usize> {
        let refresh = &self.request.refresh;
        refresh
            .ingredient
            .memo_preparation_bytes(refresh.zalsa, refresh.id)
    }

    pub(in crate::function) fn prepare(
        self,
    ) -> Result<PreparedColdCycleReady<'db, C>, (&'static str, Self)> {
        let refresh = &self.request.refresh;
        let Some(slot) = refresh.ingredient.prepare_memo_slot(
            refresh.zalsa,
            refresh.id,
            refresh.memo_ingredient_index,
        ) else {
            return Err(("memo index does not identify a registered slot", self));
        };
        let expected_root = slot.current();
        let refresh = self.request.refresh;
        let memo = PreparedMemo::new(
            self.value,
            refresh.zalsa.current_revision(),
            self.request.revisions,
        );
        Ok(PreparedColdCycleReady {
            memo,
            slot,
            expected_root,
            refresh,
        })
    }

    pub(in crate::function) fn insert(self) -> SelectionStep<'db, C> {
        match self.prepare() {
            Ok(ready) => ready.insert(None),
            Err((error, _owner)) => panic!("{error}"),
        }
    }
}

pub(in crate::function) struct PreparedColdCycleReady<'db, C: Configuration> {
    memo: PreparedMemo<'db, C>,
    slot: PreparedMemoSlot<'db, Memo<C>>,
    expected_root: Option<NonNull<Memo<C>>>,
    refresh: Refresh<'db, C>,
}

impl<'db, C: Configuration> PreparedColdCycleReady<'db, C> {
    pub(in crate::function) fn publication_work(&self) -> usize {
        1 + usize::from(self.expected_root.is_some())
    }

    pub(in crate::function) fn is_current(&self) -> bool {
        self.slot.current() == self.expected_root
    }

    pub(in crate::function) fn retirement(&self) -> PreparedRetirement<'db, C> {
        self.refresh.ingredient.deleted_entries.prepare()
    }

    pub(in crate::function) fn insert(
        self,
        retirement: Option<PreparedRetirement<'db, C>>,
    ) -> SelectionStep<'db, C> {
        #[cfg(all(test, not(feature = "shuttle")))]
        let key = self.refresh.ingredient.database_key_index(self.refresh.id);
        let memo = self
            .refresh
            .ingredient
            .install_prepared_memo(self.slot, self.memo, retirement);
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            Event::new(Kind::InitialInserted)
                .key(key)
                .memo(Some(memo.transfer_test_snapshot())),
        );
        selected(memo)
    }
}
use std::ptr::NonNull;
