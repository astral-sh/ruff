use std::ptr::NonNull;

use crate::active_query::CompletedQuery;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Action, Event as TransferEvent, Kind,
};
use crate::cycle::{CycleHeads, CycleRecoveryStrategy, IterationStamp};
use crate::function::delete::PreparedRetirement;
use crate::function::memo::{Memo, MemoHeader, MemoOutputCheck, PreparedMemo};
use crate::function::sync::ReleaseMode;
use crate::function::{
    ClaimGuard, ClaimResult, Configuration, ErasedMemo, IngredientImpl, Reentrancy,
};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::plumbing::ZalsaLocal;
use crate::sync::atomic::Ordering;
use crate::sync::thread;
use crate::table::memo::PreparedMemoSlot;
use crate::tracked_struct::Identity;
use crate::zalsa::{MemoIngredientIndex, Zalsa};
use crate::zalsa_local::{
    ActiveQueryGuard, PreparedIterationUpdate, QueryEdge, QueryEdgeKind, QueryRevisions,
};
use crate::{Cancelled, Cycle, tracing};
use crate::{DatabaseKeyIndex, Event, EventKind, Id, Revision};

pub(crate) mod execution_run;
pub(in crate::function) mod participant;
use participant::{Consumer, HeadTraversal, HeadWork, Participant, ParticipantProgress};

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    /// Executes the query function for the given claim. Creates and stores
    /// a new memo with the result, backdated if possible. Once this completes,
    /// the query will have been popped off the active query stack.
    ///
    /// # Parameters
    ///
    /// * `db`, the database.
    /// * `claim_guard`, the claim for the query to execute.
    /// * `opt_old_memo`, the older memo, if any existed. Used for backdating.
    ///
    /// # Returns
    /// The newly computed memo or `None` if this query is part of a larger cycle
    /// and `execute` blocked on a cycle head running on another thread. In this case,
    /// the memo is potentially outdated and needs to be refetched.
    #[inline(never)]
    pub(super) fn execute<'db>(
        &'db self,
        db: &'db C::DbView,
        claim_guard: ClaimGuard<'db>,
        opt_old_memo: Option<&'db Memo<C>>,
        consumer: Consumer,
    ) -> Option<&'db Memo<C>> {
        QueryExecution::start(self, db, claim_guard, opt_old_memo, consumer).execute_to_completion()
    }
}

/// Each request owns the query's claim and, after Start, its active frame until its callback finishes.
/// These states are private because dropping a request without resuming it is not an abort
/// protocol: provisional memos and transferred claims still require explicit cleanup.
#[must_use]
enum ExecutionStep<'db, C: Configuration> {
    Start(QueryExecution<'db, C>),
    Body(QueryBody<'db, C>),
    Initial(CycleInitial<'db, C>),
    RetireInitial(InitialReplacement<'db, C>),
    Recovery(CycleRecovery<'db, C>),
    ExpandHeads(ExpandHeads<'db, C>),
    CompareCycle(CycleComparison<'db, C>),
    Prepare(CompletionPreparation<'db, C>),
    Commit(PreparedQueryCommit<'db, C>),
    DidFinalize(FinalizedCycle<'db, C>),
    Participant(Participant<'db, C>),
    Complete(Option<&'db Memo<C>>),
}

// An abandoned approximation cannot seed a new attempt, but its outputs still belong
// to this query and must survive until canonical output reconciliation can retire them.
enum PreviousMemo<'db, C: Configuration> {
    Semantic(&'db Memo<C>),
    OutputsOnly(&'db Memo<C>),
}

impl<'db, C: Configuration> PreviousMemo<'db, C> {
    fn retained(&self) -> &'db Memo<C> {
        match self {
            Self::Semantic(memo) | Self::OutputsOnly(memo) => memo,
        }
    }

    fn output_header(&self) -> &'db MemoHeader {
        &self.retained().header
    }

    fn semantic(&self) -> Option<&'db Memo<C>> {
        match self {
            Self::Semantic(memo) => Some(memo),
            Self::OutputsOnly(_) => None,
        }
    }
}

pub(in crate::function) struct QueryExecution<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    db: &'db C::DbView,
    previous: Option<PreviousMemo<'db, C>>,
    memo_ingredient_index: MemoIngredientIndex,
    claim_guard: ClaimGuard<'db>,
    start_incomplete: bool,
    consumer: Consumer,
}

impl<'db, C: Configuration> QueryExecution<'db, C> {
    pub(in crate::function) fn execute_to_completion(self) -> Option<&'db Memo<C>> {
        let mut step = ExecutionStep::Start(self);
        loop {
            step = match step {
                ExecutionStep::Start(request) => request.execute(),
                ExecutionStep::Body(request) => request.execute(),
                ExecutionStep::Initial(request) => request.execute(),
                ExecutionStep::RetireInitial(request) => request.execute(),
                ExecutionStep::Recovery(request) => request.execute(),
                ExecutionStep::ExpandHeads(request) => request.execute(),
                ExecutionStep::CompareCycle(request) => request.execute(),
                ExecutionStep::Prepare(request) => request.execute(),
                ExecutionStep::Commit(request) => request.execute(),
                ExecutionStep::DidFinalize(request) => request.execute(),
                ExecutionStep::Participant(request) => match request.execute() {
                    ParticipantProgress::Pending(request) => ExecutionStep::Participant(request),
                    ParticipantProgress::Complete(memo) => ExecutionStep::Complete(memo),
                    ParticipantProgress::Execute(execution) => ExecutionStep::Start(execution),
                },
                ExecutionStep::Complete(memo) => return memo,
            }
        }
    }

    fn start(
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        claim_guard: ClaimGuard<'db>,
        opt_old_memo: Option<&'db Memo<C>>,
        consumer: Consumer,
    ) -> Self {
        let memo_ingredient_index = ingredient.memo_ingredient_index(
            claim_guard.zalsa(),
            claim_guard.database_key_index().key_index(),
        );
        Self {
            ingredient,
            db,
            previous: opt_old_memo.map(PreviousMemo::Semantic),
            memo_ingredient_index,
            claim_guard,
            start_incomplete: false,
            consumer,
        }
    }

    fn prepare_start(&mut self) {
        let database_key_index = self.claim_guard.database_key_index();
        let zalsa = self.claim_guard.zalsa();
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            TransferEvent::new(Kind::PrepareSupplied)
                .key(database_key_index)
                .serial(self.claim_guard.test_serial())
                .memo(self.previous.as_ref().map(|previous| previous.retained().transfer_test_snapshot())),
        );

        if let Some(old_memo) = self.previous.as_ref().map(PreviousMemo::retained)
            && old_memo.value.is_none()
            && old_memo.header.may_be_provisional()
            && old_memo.header.verified_at.load() == zalsa.current_revision()
            && old_memo.header.revisions.iteration().cancellation_count()
                == zalsa.runtime().cancellation_count()
        {
            Cancelled::PropagatedPanic.throw();
        }
        // Reject the old approximation and backdating baseline without losing its output owner.
        if let Some(previous @ PreviousMemo::Semantic(_)) = &mut self.previous
            && !previous.retained().header.can_seed_attempt(zalsa)
        {
            *previous = PreviousMemo::OutputsOnly(previous.retained());
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            TransferEvent::new(Kind::PrepareRetained)
                .key(database_key_index)
                .serial(self.claim_guard.test_serial())
                .memo(self.previous.as_ref().and_then(PreviousMemo::semantic).map(Memo::transfer_test_snapshot)),
        );

        crate::tracing::info!("{:?}: executing query", database_key_index);

        let ((), observed) = crate::attempt_probe::with_incomplete_observation(|| {
            zalsa.event(&|| {
                Event::new(EventKind::WillExecute {
                    database_key: database_key_index,
                })
            });
        });
        self.start_incomplete = observed;
    }

    fn execute(mut self) -> ExecutionStep<'db, C> {
        self.prepare_start();
        self.resume_start()
    }

    fn resume_start(self) -> ExecutionStep<'db, C> {
        let database_key_index = self.claim_guard.database_key_index();
        let opt_old_memo = self.previous.as_ref().and_then(PreviousMemo::semantic);
        match C::CYCLE_STRATEGY {
            CycleRecoveryStrategy::Panic => {
                let active_query = self
                    .claim_guard
                    .zalsa_local()
                    .push_query(database_key_index);
                QueryBody::start(
                    active_query,
                    BodyContinuation::Ordinary(self),
                    opt_old_memo.map(|memo| &memo.header),
                )
            }
            CycleRecoveryStrategy::FallbackImmediate | CycleRecoveryStrategy::Fixpoint => {
                IteratingQuery::new(self).next_body()
            }
        }
    }

    fn complete(
        self,
        new_value: C::Output<'db>,
        completed_query: CompletedQuery,
        cancellation_guard: Option<DisableLocalCancellationGuard<'db>>,
    ) -> ExecutionStep<'db, C> {
        ExecutionStep::Prepare(CompletionPreparation {
            value: new_value,
            completed: completed_query,
            disposition: CompletionDisposition::Return {
                cancellation_guard,
                execution: self,
            },
        })
    }

    fn after_body(
        self,
        active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
    ) -> ExecutionStep<'db, C> {
        if active_query.attempt_incomplete() {
            active_query.finish_attempt_incomplete();
        }

        // Ordinary queries don't need a cycle iteration stamp. Keeping the default avoids
        // allocating `QueryRevisionsExtra` after a revision-preserving cancellation.
        let completed = active_query.pop(IterationStamp::default());
        let mask = (!completed.revisions.cycle_heads().is_empty())
            .then(|| DisableLocalCancellationGuard::new(self.claim_guard.zalsa_local()));
        self.complete(new_value, completed, mask)
    }
}

struct CompletionPreparation<'db, C: Configuration> {
    // A rejected completion destroys its value before releasing the original query owner.
    value: C::Output<'db>,
    completed: CompletedQuery,
    disposition: CompletionDisposition<'db, C>,
}

enum CompletionPreparationStage<'db, C: Configuration> {
    Complete,
    Backdate {
        old_value: Option<&'db C::Output<'db>>,
    },
}

enum CompletionDisposition<'db, C: Configuration> {
    Return {
        // Cycle participants retain their Local mask through publication and claim transfer.
        cancellation_guard: Option<DisableLocalCancellationGuard<'db>>,
        execution: QueryExecution<'db, C>,
    },
    Repeat {
        query: IteratingQuery<'db, C>,
        cycle_heads: CycleHeads,
        next_iteration: IterationStamp,
    },
    Finalize {
        // On unwind, poison the provisional result, restore Local, then release the claim.
        poison_guard: PoisonProvisionalIfPanicking<'db, C>,
        cancellation_guard: DisableLocalCancellationGuard<'db>,
        execution: QueryExecution<'db, C>,
        cycle_heads: CycleHeads,
        iteration: IterationStamp,
    },
}

impl<'db, C: Configuration> CompletionDisposition<'db, C> {
    fn execution(&self) -> &QueryExecution<'db, C> {
        match self {
            Self::Return { execution, .. } | Self::Finalize { execution, .. } => execution,
            Self::Repeat { query, .. } => &query.execution,
        }
    }
}

impl<'db, C: Configuration> CompletionPreparation<'db, C> {
    fn output_check_work(&self) -> Option<usize> {
        let check = MemoOutputCheck::Controlled;
        let completed = check.work(&self.completed.revisions);
        let old = self
            .disposition
            .execution()
            .previous
            .as_ref()
            .map_or(0, |previous| {
                previous.output_header().output_check_work(check)
            });
        let mut entry = completed.checked_add(old)?;
        if let CompletionDisposition::Repeat { query, .. } = &self.disposition {
            entry = entry.checked_add(
                query
                    .opt_old_memo
                    .map_or(0, |memo| memo.header.output_check_work(check)),
            )?;
            entry = entry.checked_add(
                query
                    .last_provisional_memo_opt
                    .map_or(0, |memo| memo.header.output_check_work(check)),
            )?;
        }

        // Each preparation phase returns after its first successful mark_refused. Commit
        // can repeat it once; every pass also checks the completed origin when marking it.
        let before_comparison =
            entry.checked_add(self.completed.revisions.origin().output_scan_work())?;
        let after_comparison = before_comparison;
        let commit = before_comparison;
        entry
            .checked_add(before_comparison)?
            .checked_add(after_comparison)?
            .checked_add(commit)
    }

    fn controlled_outputs_are_empty(&self) -> bool {
        fn empty(revisions: &QueryRevisions) -> bool {
            #[cfg(feature = "accumulator")]
            if revisions.accumulated().is_some() {
                return false;
            }
            revisions.tracked_struct_ids().is_empty()
                && revisions.origin().outputs().next().is_none()
        }

        empty(&self.completed.revisions)
            && self.completed.stale_tracked_structs.is_empty()
            && self
                .disposition
                .execution()
                .previous
                .as_ref()
                .is_none_or(|previous| empty(&previous.output_header().revisions))
            && match &self.disposition {
                CompletionDisposition::Return { .. } | CompletionDisposition::Finalize { .. } => {
                    true
                }
                CompletionDisposition::Repeat { query, .. } => {
                    query.last_stale_tracked_ids.is_empty()
                        && query
                            .opt_old_memo
                            .is_none_or(|memo| empty(&memo.header.revisions))
                        && query
                            .last_provisional_memo_opt
                            .is_none_or(|memo| empty(&memo.header.revisions))
                }
            }
    }

    fn mark_refused(&mut self, observed: bool) -> bool {
        // A pending value still depends on its provisional support, even if its
        // local cycle has converged. Ambient refusal does not taint other work.
        let pending_incomplete = self
            .completed
            .revisions
            .attempt_support()
            .is_some_and(|support| support.incomplete(true));
        if !observed && !pending_incomplete {
            return false;
        }
        let Some(support) = crate::attempt_probe::current_query() else {
            return false;
        };
        assert!(
            self.controlled_outputs_are_empty(),
            "incomplete return-only completion retained tracked outputs"
        );
        self.completed.revisions.finish_attempt_incomplete(&support);
        true
    }

    fn prepare_before_comparison(&mut self) -> CompletionPreparationStage<'db, C> {
        if self.mark_refused(false) {
            return CompletionPreparationStage::Complete;
        }
        let execution = self.disposition.execution();
        let database_key_index = execution.claim_guard.database_key_index();
        let zalsa = execution.claim_guard.zalsa();
        if let CompletionDisposition::Repeat { next_iteration, .. } = &self.disposition {
            let ((), observed) = crate::attempt_probe::with_incomplete_observation(|| {
                zalsa.event(&|| {
                    Event::new(EventKind::WillIterateCycle {
                        database_key: database_key_index,
                        iteration: next_iteration.iteration(),
                    })
                });
            });
            self.mark_refused(observed);
            return CompletionPreparationStage::Complete;
        }

        CompletionPreparationStage::Backdate {
            old_value: execution
                .previous
                .as_ref()
                .and_then(PreviousMemo::semantic)
                .and_then(|old_memo| {
                old_memo
                    .backdate_comparison(&self.completed.revisions, &self.value)
                    .map(|(old_value, _)| old_value)
            }),
        }
    }

    fn comparison_values(
        &self,
        stage: &CompletionPreparationStage<'db, C>,
    ) -> Option<(&C::Output<'db>, &C::Output<'db>)> {
        match stage {
            CompletionPreparationStage::Backdate {
                old_value: Some(old_value),
            } => Some((old_value, &self.value)),
            CompletionPreparationStage::Complete
            | CompletionPreparationStage::Backdate { old_value: None } => None,
        }
    }

    fn prepare_after_comparison(&mut self, stage: CompletionPreparationStage<'db, C>) {
        let CompletionPreparationStage::Backdate { old_value } = stage else {
            return;
        };
        if self.mark_refused(false) {
            return;
        }
        let execution = self.disposition.execution();
        let database_key_index = execution.claim_guard.database_key_index();
        let changed_at = self.completed.revisions.changed_at;
        if let Some(old_memo) = execution.previous.as_ref().and_then(PreviousMemo::semantic) {
            // If the new value is equal to the old one, then it didn't
            // really change, even if some of its inputs have. So we can
            // "backdate" its `changed_at` revision to be the same as the
            // old value.
            let ((), observed) = crate::attempt_probe::with_incomplete_observation(|| {
                // Admission can suspend while this memo's eligibility changes. Recheck the
                // shared selection, but never begin a comparison that was not selected.
                if old_value.is_some() {
                    execution.ingredient.backdate_if_appropriate(
                        old_memo,
                        database_key_index,
                        &mut self.completed.revisions,
                        &self.value,
                    );
                }
            });
            if self.mark_refused(observed) {
                self.completed.revisions.changed_at = changed_at;
                return;
            }
        }

        if let Some(old_header) = self
            .disposition
            .execution()
            .previous
            .as_ref()
            .map(PreviousMemo::output_header)
        {
            // Diff the new outputs with the old, to discard any no-longer-emitted
            // outputs and update the tracked struct IDs for seeding the next revision.
            let ((), observed) = crate::attempt_probe::with_incomplete_observation(|| {
                old_header.diff_outputs(
                    self.disposition.execution().claim_guard.zalsa(),
                    database_key_index,
                    &self.completed,
                );
            });
            if self.mark_refused(observed) {
                self.completed.revisions.changed_at = changed_at;
                return;
            }
        }

        #[cfg(not(feature = "persistence"))]
        self.completed.revisions.discard_edges_if_never_change();
    }

    fn prepare(&mut self) {
        let stage = self.prepare_before_comparison();
        self.prepare_after_comparison(stage);
    }

    fn execute(mut self) -> ExecutionStep<'db, C> {
        self.prepare();
        match self.into_commit() {
            Ok(commit) => ExecutionStep::Commit(commit),
            Err((error, _owner)) => panic!("{error}"),
        }
    }

    fn target_capacity(&self) -> usize {
        let me = self
            .disposition
            .execution()
            .claim_guard
            .database_key_index();
        match &self.disposition {
            CompletionDisposition::Return { .. } => 0,
            CompletionDisposition::Repeat { cycle_heads, .. }
            | CompletionDisposition::Finalize { cycle_heads, .. } => {
                cycle_heads.iter_not_eq(me).count()
            }
        }
    }

    fn storage_bytes(&self) -> Option<usize> {
        let execution = self.disposition.execution();
        let participant_entries = self.participant_entries();
        let participant_bytes = if participant_entries == 0 {
            0
        } else {
            participant_entries
                .checked_mul(size_of::<crate::cycle::CycleHead>())?
                .checked_add(2 * size_of::<usize>())?
        };
        execution
            .ingredient
            .memo_preparation_bytes(
                execution.claim_guard.zalsa(),
                execution.claim_guard.database_key_index().key_index(),
            )?
            .checked_add(participant_bytes)?
            .checked_add(
                self.target_capacity()
                    .checked_mul(size_of::<CommitTarget<'db>>())?,
            )
    }

    fn participant_entries(&self) -> usize {
        if matches!(self.disposition, CompletionDisposition::Return { .. }) {
            self.completed.revisions.cycle_heads().storage_len()
        } else {
            0
        }
    }

    fn prepare_targets(&self) -> Result<Vec<CommitTarget<'db>>, &'static str> {
        let execution = self.disposition.execution();
        let zalsa = execution.claim_guard.zalsa();
        let me = execution.claim_guard.database_key_index();
        let mut targets = Vec::with_capacity(self.target_capacity());
        let heads = match &self.disposition {
            CompletionDisposition::Return { .. } => None,
            CompletionDisposition::Repeat { cycle_heads, .. }
            | CompletionDisposition::Finalize { cycle_heads, .. } => Some(cycle_heads),
        };
        if let Some(heads) = heads {
            for head in heads.iter_not_eq(me) {
                let key = head.database_key_index;
                let Some(function) = zalsa
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                else {
                    return Err("cycle head is not a function ingredient");
                };
                let Some(memo) = function.memo(zalsa, key.key_index()) else {
                    targets.push(CommitTarget {
                        key,
                        selected: None,
                        write: TargetWrite::Absent,
                    });
                    continue;
                };
                let write = if let CompletionDisposition::Repeat { next_iteration, .. } =
                    &self.disposition
                {
                    TargetWrite::Iteration(
                        memo.header()
                            .revisions
                            .prepare_iteration_count(key, *next_iteration)?,
                    )
                } else {
                    if memo.header().has_incomplete_attempt() {
                        return Err("incomplete cycle cannot be finalized");
                    }
                    TargetWrite::Finalize
                };
                targets.push(CommitTarget {
                    key,
                    selected: Some((
                        memo,
                        memo.header().verified_at.load(),
                        memo.header().revisions.iteration(),
                    )),
                    write,
                });
            }
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        for target in &targets {
            target.transfer_test_record(Kind::Target, true);
        }
        Ok(targets)
    }

    fn into_commit(mut self) -> Result<PreparedQueryCommit<'db, C>, (&'static str, Self)> {
        if self.mark_refused(false) {
            // Native calls carry a typed incomplete value back to their caller. A
            // refused preparation must retire its iteration instead of retaining an
            // unusable predecessor or publishing another cycle member's final flag.
            let (mut execution, cancellation_guard) = match self.disposition {
                CompletionDisposition::Return {
                    cancellation_guard,
                    execution,
                } => (execution, cancellation_guard),
                CompletionDisposition::Repeat { query, .. } => {
                    let IteratingQuery {
                        poison_guard,
                        cancellation_guard,
                        last_stale_tracked_ids,
                        execution,
                        ..
                    } = query;
                    drop(poison_guard);
                    drop(last_stale_tracked_ids);
                    (execution, Some(cancellation_guard))
                }
                CompletionDisposition::Finalize {
                    poison_guard,
                    cancellation_guard,
                    execution,
                    ..
                } => {
                    drop(poison_guard);
                    (execution, Some(cancellation_guard))
                }
            };
            execution.claim_guard.set_release_mode(ReleaseMode::Default);
            self.disposition = CompletionDisposition::Return {
                cancellation_guard,
                execution,
            };
        }
        let execution = self.disposition.execution();
        let zalsa = execution.claim_guard.zalsa();
        let me = execution.claim_guard.database_key_index();
        let Some(slot) = execution.ingredient.prepare_memo_slot(
            zalsa,
            me.key_index(),
            execution.memo_ingredient_index,
        ) else {
            return Err(("memo index does not identify a registered slot", self));
        };
        let expected_root = slot.current();
        let targets = match self.prepare_targets() {
            Ok(targets) => targets,
            Err(error) => return Err((error, self)),
        };
        if let CompletionDisposition::Repeat {
            cycle_heads,
            next_iteration,
            ..
        } = &mut self.disposition
        {
            debug_assert!(self.completed.revisions.cycle_heads().is_empty());
            cycle_heads.update_iteration_count_mut(me, *next_iteration);
            // `complete_cycle_query` forces extra metadata when popping the query, so this
            // moves its existing heads without allocating a second metadata block.
            self.completed
                .revisions
                .set_cycle_heads(std::mem::take(cycle_heads), *next_iteration);
            *self.completed.revisions.verified_final.get_mut() = false;
        }
        // Keep the owner in an earlier local: allocation or metadata failures retire the
        // still-owned output before unwinding through its claim and provisional guards.
        let participant_entries = self.participant_entries();
        let disposition = self.disposition;
        let memo = PreparedMemo::new(
            self.value,
            zalsa.current_revision(),
            self.completed.revisions,
        );
        Ok(PreparedQueryCommit {
            participant_entries,
            memo,
            targets,
            stale_tracked_structs: self.completed.stale_tracked_structs,
            slot,
            expected_root,
            disposition,
        })
    }
}

enum TargetWrite<'db> {
    Absent,
    Iteration(Option<PreparedIterationUpdate<'db>>),
    Finalize,
}

struct CommitTarget<'db> {
    key: DatabaseKeyIndex,
    selected: Option<(ErasedMemo<'db>, Revision, IterationStamp)>,
    write: TargetWrite<'db>,
}

impl CommitTarget<'_> {
    #[cfg(all(test, not(feature = "shuttle")))]
    fn transfer_test_record(&self, kind: Kind, decision: bool) {
        let mut event = TransferEvent::new(kind)
            .key(self.key)
            .decision(decision)
            .memo(
                self.selected
                    .map(|(memo, ..)| memo.transfer_test_snapshot()),
            );
        event.action = Some(match &self.write {
            TargetWrite::Absent => Action::Absent,
            TargetWrite::Iteration(_) => Action::Iteration,
            TargetWrite::Finalize => Action::Finalize,
        });
        transfer_trace::record(event);
    }

    fn is_current(&self, zalsa: &Zalsa) -> bool {
        let result = (|| {
            let Some(function) = zalsa
                .lookup_ingredient(self.key.ingredient_index())
                .as_function()
            else {
                return false;
            };
            let current = function.memo(zalsa, self.key.key_index());
            let Some((memo, revision, iteration)) = self.selected else {
                return current.is_none();
            };
            current.is_some_and(|current| std::ptr::eq(current.header(), memo.header()))
                && memo.header().verified_at.load() == revision
                && memo.header().revisions.iteration() == iteration
                && match &self.write {
                    TargetWrite::Absent => false,
                    TargetWrite::Iteration(update) => update
                        .as_ref()
                        .is_none_or(PreparedIterationUpdate::is_current),
                    TargetWrite::Finalize => !memo.header().has_incomplete_attempt(),
                }
        })();
        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(Kind::TargetCurrent, result);
        result
    }

    fn writes(&self) -> usize {
        match &self.write {
            TargetWrite::Absent => 0,
            TargetWrite::Iteration(update) => {
                update.as_ref().map_or(0, PreparedIterationUpdate::writes)
            }
            TargetWrite::Finalize => 1,
        }
    }

    fn publish(self) {
        #[cfg(all(test, not(feature = "shuttle")))]
        let action = match &self.write {
            TargetWrite::Absent => Action::Absent,
            TargetWrite::Iteration(_) => Action::Iteration,
            TargetWrite::Finalize => Action::Finalize,
        };
        match self.write {
            TargetWrite::Absent => {}
            TargetWrite::Iteration(Some(update)) => update.publish(),
            TargetWrite::Iteration(None) => {}
            TargetWrite::Finalize => {
                if let Some((memo, ..)) = self.selected {
                    memo.header()
                        .revisions
                        .verified_final
                        .store(true, Ordering::Release);
                }
            }
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = TransferEvent::new(Kind::TargetPublished)
                .key(self.key)
                .memo(
                    self.selected
                        .map(|(memo, ..)| memo.transfer_test_snapshot()),
                );
            event.action = Some(action);
            transfer_trace::record(event);
        }
    }
}

struct PreparedQueryCommit<'db, C: Configuration> {
    participant_entries: usize,
    memo: PreparedMemo<'db, C>,
    targets: Vec<CommitTarget<'db>>,
    stale_tracked_structs: Vec<(Identity, Id)>,
    slot: PreparedMemoSlot<'db, Memo<C>>,
    expected_root: Option<NonNull<Memo<C>>>,
    disposition: CompletionDisposition<'db, C>,
}

impl<'db, C: Configuration> PreparedQueryCommit<'db, C> {
    fn publication_work(&self) -> Option<usize> {
        self.targets.iter().try_fold(
            self.participant_entries
                .checked_mul(8)?
                .checked_add(1 + usize::from(self.expected_root.is_some()))?,
            |total, target| total.checked_add(target.writes()),
        )
    }

    fn is_current(&self) -> bool {
        let result = self.slot.current() == self.expected_root
            && self
                .targets
                .iter()
                .all(|target| target.is_current(self.disposition.execution().claim_guard.zalsa()));
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            TransferEvent::new(Kind::CommitCurrent)
                .key(
                    self.disposition
                        .execution()
                        .claim_guard
                        .database_key_index(),
                )
                .serial(self.disposition.execution().claim_guard.test_serial())
                .decision(result),
        );
        result
    }

    fn retirement(&self) -> PreparedRetirement<'db, C> {
        self.disposition
            .execution()
            .ingredient
            .deleted_entries
            .prepare()
    }

    fn execute(self) -> ExecutionStep<'db, C> {
        assert!(
            self.is_current(),
            "prepared publication changed its selected memos"
        );
        self.publish(None)
    }

    fn publish(self, retirement: Option<PreparedRetirement<'db, C>>) -> ExecutionStep<'db, C> {
        let Self {
            memo,
            targets,
            stale_tracked_structs,
            slot,
            disposition,
            ..
        } = self;
        match disposition {
            // Destructured locals unwind in reverse binding order, unlike enum fields.
            CompletionDisposition::Return {
                execution,
                cancellation_guard,
            } => {
                #[cfg(all(test, not(feature = "shuttle")))]
                let (key, serial) = (
                    execution.claim_guard.database_key_index(),
                    execution.claim_guard.test_serial(),
                );
                let memo = execution
                    .ingredient
                    .install_prepared_memo(slot, memo, retirement);
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::RootPublished)
                        .key(key)
                        .serial(serial)
                        .memo(Some(memo.transfer_test_snapshot())),
                );
                if memo.header.may_be_provisional()
                    && memo.header.attempt_reuse(execution.claim_guard.zalsa())
                        == crate::attempt_probe::MemoReuse::Ordinary
                {
                    return ExecutionStep::Participant(Participant::published(
                        execution,
                        memo,
                        cancellation_guard,
                    ));
                }
                let refetch = execution.claim_guard.drop();
                drop(cancellation_guard);
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::Refetch)
                        .key(key)
                        .serial(serial)
                        .decision(refetch),
                );
                ExecutionStep::Complete(if refetch { None } else { Some(memo) })
            }
            CompletionDisposition::Repeat {
                mut query,
                next_iteration,
                ..
            } => {
                for target in targets {
                    target.publish();
                }
                let memo = query
                    .execution
                    .ingredient
                    .install_prepared_memo(slot, memo, retirement);
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::RootPublished)
                        .key(query.execution.claim_guard.database_key_index())
                        .serial(query.execution.claim_guard.test_serial())
                        .memo(Some(memo.transfer_test_snapshot())),
                );
                query.iteration = next_iteration;
                query.last_provisional_memo_opt = Some(memo);
                query.last_stale_tracked_ids = stale_tracked_structs;
                query.next_body()
            }
            CompletionDisposition::Finalize {
                execution,
                cancellation_guard,
                poison_guard,
                iteration,
                ..
            } => {
                let key = execution.claim_guard.database_key_index();
                let zalsa = execution.claim_guard.zalsa();
                #[cfg(all(test, not(feature = "shuttle")))]
                let serial = execution.claim_guard.test_serial();
                let memo = execution
                    .ingredient
                    .install_prepared_memo(slot, memo, retirement);
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::RootPublished)
                        .key(key)
                        .serial(serial)
                        .memo(Some(memo.transfer_test_snapshot())),
                );
                drop(poison_guard);
                // Root publication precedes every nested final flag. Targets were checked
                // before this interval; these stores cannot call out or reject the group.
                for target in targets {
                    target.publish();
                }
                // Native waiter release is outside the allocation-free store interval and
                // must complete before a failing event can unwind through this execution.
                let refetch = execution.claim_guard.drop();
                drop(cancellation_guard);
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::Refetch)
                        .key(key)
                        .serial(serial)
                        .decision(refetch),
                );
                ExecutionStep::DidFinalize(FinalizedCycle {
                    memo: if refetch { None } else { Some(memo) },
                    zalsa,
                    key,
                    iteration,
                })
            }
        }
    }
}

struct FinalizedCycle<'db, C: Configuration> {
    memo: Option<&'db Memo<C>>,
    zalsa: &'db Zalsa,
    key: DatabaseKeyIndex,
    iteration: IterationStamp,
}

impl<'db, C: Configuration> FinalizedCycle<'db, C> {
    fn event(&self) {
        self.zalsa.event(&|| {
            Event::new(EventKind::DidFinalizeCycle {
                database_key: self.key,
                iteration: self.iteration.iteration(),
            })
        });
    }

    fn execute(self) -> ExecutionStep<'db, C> {
        self.event();
        ExecutionStep::Complete(self.memo)
    }
}

enum BodyContinuation<'db, C: Configuration> {
    Ordinary(QueryExecution<'db, C>),
    Iterating(IteratingQuery<'db, C>),
}

impl<'db, C: Configuration> BodyContinuation<'db, C> {
    fn execution(&self) -> &QueryExecution<'db, C> {
        match self {
            Self::Ordinary(execution) => execution,
            Self::Iterating(query) => &query.execution,
        }
    }
}

struct QueryBody<'db, C: Configuration> {
    // Field order ensures that unwinding removes the active frame before releasing its claim.
    active_query: ActiveQueryGuard<'db>,
    continuation: BodyContinuation<'db, C>,
}

impl<'db, C: Configuration> QueryBody<'db, C> {
    fn start(
        active_query: ActiveQueryGuard<'db>,
        continuation: BodyContinuation<'db, C>,
        opt_old_header: Option<&MemoHeader>,
    ) -> ExecutionStep<'db, C> {
        let request = Self {
            active_query,
            continuation,
        };
        let execution = request.continuation.execution();
        let zalsa = execution.claim_guard.zalsa();
        let seeded = opt_old_header
            .is_some_and(|header| header.seed_active_query(zalsa, &request.active_query));
        if !seeded
            && let Some(header) = opt_old_header
                .or_else(|| execution.previous.as_ref().map(PreviousMemo::output_header))
        {
            if header.revisions.execution_revision() == Some(zalsa.current_revision()) {
                request.active_query.seed_output_ownership(
                    header.revisions.origin().edges(),
                    header.revisions.tracked_struct_ids(),
                );
            } else {
                request
                    .active_query
                    .seed_tracked_struct_ids(header.revisions.tracked_struct_ids());
            }
        }
        // WillExecute runs before this frame exists. Only incomplete use in that
        // callback belongs to this body; an earlier refusal can be independent.
        if request.continuation.execution().start_incomplete {
            request
                .continuation
                .execution()
                .claim_guard
                .zalsa_local()
                .mark_attempt_incomplete(request.continuation.execution().claim_guard.zalsa());
        }
        ExecutionStep::Body(request)
    }

    fn execute(self) -> ExecutionStep<'db, C> {
        let execution = self.continuation.execution();
        // Query was not previously executed, or value is potentially
        // stale, or value is absent. Let's execute!
        let new_value = C::execute(
            execution.db,
            C::id_to_input(
                execution.claim_guard.zalsa(),
                self.active_query.database_key_index.key_index(),
            ),
        );
        self.resume(new_value)
    }

    fn resume(self, new_value: C::Output<'db>) -> ExecutionStep<'db, C> {
        match self.continuation {
            BodyContinuation::Ordinary(execution) => {
                execution.after_body(self.active_query, new_value)
            }
            BodyContinuation::Iterating(query) => query.after_body(self.active_query, new_value),
        }
    }
}

struct IteratingQuery<'db, C: Configuration> {
    // These guards span every iteration. Completion retains Local masking until its claim retires;
    // the poison guard follows the completion's disposition. Keep their unwind drop order:
    // poison provisional values, restore cancellation, then release the claim in `execution`.
    poison_guard: PoisonProvisionalIfPanicking<'db, C>,
    cancellation_guard: DisableLocalCancellationGuard<'db>,
    // Our provisional value from the previous iteration, when doing fixpoint iteration.
    // This is different from `opt_old_memo` which might be from a different revision.
    last_provisional_memo_opt: Option<&'db Memo<C>>,
    opt_old_memo: Option<&'db Memo<C>>,
    last_stale_tracked_ids: Vec<(Identity, Id)>,
    iteration: IterationStamp,
    execution: QueryExecution<'db, C>,
}

impl<'db, C: Configuration> IteratingQuery<'db, C> {
    fn new(mut execution: QueryExecution<'db, C>) -> Self {
        let cancellation_guard =
            DisableLocalCancellationGuard::new(execution.claim_guard.zalsa_local());
        execution.claim_guard.set_release_mode(ReleaseMode::Default);
        let database_key_index = execution.claim_guard.database_key_index();
        let zalsa = execution.claim_guard.zalsa();
        let id = database_key_index.key_index();
        let current_revision = zalsa.current_revision();
        let cancellation_count = zalsa.runtime().cancellation_count();
        let mut last_provisional_memo_opt = None;
        let mut opt_old_memo = execution.previous.as_ref().and_then(PreviousMemo::semantic);
        let mut iteration = IterationStamp::initial(cancellation_count);

        // An ordinary query doesn't memoize a cancelled execution. Match that behavior for
        // fixpoint queries: a memo from an abandoned cancellation epoch in this revision doesn't
        // seed the retry, while a memo from an older revision remains useful for backdating and
        // output bookkeeping. Cancellation counts are only comparable within a revision.
        if let Some(old_memo) = opt_old_memo
            && old_memo.header.verified_at.load() == current_revision
        {
            match old_memo.header.previous_iteration(
                zalsa,
                database_key_index,
                cancellation_count,
                old_memo.value.is_some(),
            ) {
                Some(previous_iteration) => {
                    if previous_iteration.reuse_as_provisional {
                        last_provisional_memo_opt = Some(old_memo);
                    }
                    iteration = previous_iteration.iteration;
                }
                None => opt_old_memo = None,
            }
        }

        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = TransferEvent::new(Kind::Iteration)
                .key(database_key_index)
                .serial(execution.claim_guard.test_serial())
                .memo(opt_old_memo.map(Memo::transfer_test_snapshot));
            event.other_memo = last_provisional_memo_opt.map(Memo::transfer_test_snapshot);
            event.iteration = Some(iteration);
            transfer_trace::record(event);
        }
        Self {
            poison_guard: PoisonProvisionalIfPanicking::new(
                execution.ingredient,
                zalsa,
                id,
                execution.memo_ingredient_index,
            ),
            cancellation_guard,
            last_provisional_memo_opt,
            opt_old_memo,
            last_stale_tracked_ids: Vec::new(),
            iteration,
            execution,
        }
    }

    fn next_body(self) -> ExecutionStep<'db, C> {
        let active_query = self
            .execution
            .claim_guard
            .zalsa_local()
            .push_query(self.execution.claim_guard.database_key_index());

        // Tracked struct ids that existed in the previous revision
        // but weren't recreated in the last iteration. It's important that we seed the next
        // query with these ids because the query might re-create them as part of the next iteration.
        // This is not only important to ensure that the re-created tracked structs have the same ids,
        // it's also important to ensure that these tracked structs get removed
        // if they aren't recreated when reaching the final iteration.
        active_query.seed_tracked_struct_ids(&self.last_stale_tracked_ids);
        let old_header = self
            .last_provisional_memo_opt
            .or(self.opt_old_memo)
            .map(|memo| &memo.header);
        QueryBody::start(active_query, BodyContinuation::Iterating(self), old_header)
    }

    fn after_body(
        self,
        active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
    ) -> ExecutionStep<'db, C> {
        if active_query.attempt_incomplete() {
            return self.finish_incomplete(active_query, new_value);
        }
        let mut active_query = active_query;
        let heads = active_query.take_cycle_heads();
        if heads.is_empty() {
            let iteration = self.iteration;
            return self.after_heads(active_query, new_value, heads, iteration, false);
        }
        let traversal = HeadTraversal::new(heads, active_query.database_key_index, self.iteration);
        ExecutionStep::ExpandHeads(ExpandHeads {
            new_value,
            active_query,
            query: self,
            traversal,
        })
    }

    fn after_heads(
        mut self,
        active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
        heads: CycleHeads,
        maximum: IterationStamp,
        depends_on_self: bool,
    ) -> ExecutionStep<'db, C> {
        let zalsa = self.execution.claim_guard.zalsa();
        match try_complete_query(
            zalsa,
            active_query,
            &mut self.execution.claim_guard,
            self.iteration,
            heads,
            maximum,
            depends_on_self,
        ) {
            QueryExecutionOutcome::Completed(completed_query) => {
                self.complete(new_value, completed_query)
            }
            QueryExecutionOutcome::Participant {
                active_query,
                cycle_heads,
                outer_cycle,
            } => {
                // For FallbackImmediate, use the fallback value instead of the computed value
                // for all cycle participants. This ensures that the results don't depend on the query call order, see
                // https://github.com/salsa-rs/salsa/pull/798#issuecomment-2812855285.
                let participant = CycleParticipant {
                    cycle_heads,
                    outer_cycle,
                    query: self,
                };
                if C::CYCLE_STRATEGY == CycleRecoveryStrategy::FallbackImmediate {
                    ExecutionStep::Initial(CycleInitial {
                        active_query,
                        computed_value: new_value,
                        continuation: InitialContinuation::Participant(participant),
                    })
                } else {
                    participant.resume(active_query, new_value)
                }
            }
            QueryExecutionOutcome::CycleHead {
                active_query,
                cycle_heads,
                outer_cycle,
                cycle_iteration,
            } => {
                let database_key_index = self.execution.claim_guard.database_key_index();
                let id = database_key_index.key_index();
                // Get the last provisional value for this query so that we can compare it with the new value
                // to test if the cycle converged.
                let last_provisional_memo = self.last_provisional_memo_opt.unwrap_or_else(|| {
                    // This is our first time around the loop; a provisional value must have been
                    // inserted into the memo table when the cycle was hit, so let's pull our
                    // initial provisional value from there.
                    let memo = self.execution.ingredient
                        .get_memo_from_table_for(zalsa, id, self.execution.memo_ingredient_index)
                        .unwrap_or_else(|| {
                            unreachable!("{database_key_index:#?} is a cycle head, but no provisional memo found")
                        });
                    debug_assert!(memo.header.may_be_provisional());
                    memo
                });
                let last_provisional_value = last_provisional_memo.value().expect(
                    "`fetch_cold_cycle` should have inserted a provisional memo with Cycle::initial",
                );
                tracing::debug!(
                    "{database_key_index:?}: execute: \
                    I am a cycle head, comparing last provisional value with new value"
                );
                let head = CycleHead {
                    cycle_heads,
                    outer_cycle,
                    cycle_iteration,
                    last_provisional_memo,
                    last_provisional_value,
                    query: self,
                };

                // For FallbackImmediate, the value always converges immediately (we use the
                // fallback directly). We still iterate if metadata hasn't converged.
                // For Fixpoint, ask the recovery function what value to use and check convergence.
                if C::CYCLE_STRATEGY == CycleRecoveryStrategy::FallbackImmediate {
                    // Use the fallback value instead of the computed value.
                    ExecutionStep::Initial(CycleInitial {
                        active_query,
                        computed_value: new_value,
                        continuation: InitialContinuation::Head(head),
                    })
                } else {
                    ExecutionStep::Recovery(CycleRecovery {
                        continuation: RecoveryContinuation { active_query, head },
                        new_value,
                    })
                }
            }
        }
    }

    fn finish_incomplete(
        mut self,
        active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
    ) -> ExecutionStep<'db, C> {
        let completed_query = finish_incomplete_query(
            active_query,
            &mut self.execution.claim_guard,
            self.iteration,
        );
        self.complete(new_value, completed_query)
    }

    fn complete(
        self,
        new_value: C::Output<'db>,
        completed_query: CompletedQuery,
    ) -> ExecutionStep<'db, C> {
        let database_key_index = self.execution.claim_guard.database_key_index();
        tracing::debug!(
            "{database_key_index:?}: execute_maybe_iterate: result.revisions = {revisions:#?}",
            revisions = &completed_query.revisions
        );
        // A completed participant no longer owns the cycle's failure state. Retire its poison
        // guard before output diffing, but retain Local masking through publication and transfer.
        drop(self.poison_guard);
        drop(self.last_stale_tracked_ids);
        self.execution
            .complete(new_value, completed_query, Some(self.cancellation_guard))
    }
}

struct CycleParticipant<'db, C: Configuration> {
    cycle_heads: CycleHeads,
    outer_cycle: DatabaseKeyIndex,
    query: IteratingQuery<'db, C>,
}

impl<'db, C: Configuration> CycleParticipant<'db, C> {
    fn resume(
        mut self,
        active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
    ) -> ExecutionStep<'db, C> {
        if active_query.attempt_incomplete() {
            return self.query.finish_incomplete(active_query, new_value);
        }
        let completed_query = complete_cycle_participant(
            active_query,
            &mut self.query.execution.claim_guard,
            self.cycle_heads,
            self.outer_cycle,
            self.query.iteration,
        );
        self.query.complete(new_value, completed_query)
    }
}

struct CycleHead<'db, C: Configuration> {
    cycle_heads: CycleHeads,
    outer_cycle: Option<DatabaseKeyIndex>,
    cycle_iteration: IterationStamp,
    last_provisional_memo: &'db Memo<C>,
    last_provisional_value: &'db C::Output<'db>,
    query: IteratingQuery<'db, C>,
}

impl<'db, C: Configuration> CycleHead<'db, C> {
    fn compared(
        mut self,
        mut active_query: ActiveQueryGuard<'db>,
        new_value: C::Output<'db>,
        value_converged: bool,
    ) -> ExecutionStep<'db, C> {
        let database_key_index = self.query.execution.claim_guard.database_key_index();
        let new_cycle_heads = active_query.take_cycle_heads();
        assert_no_new_cycle_heads(&self.cycle_heads, new_cycle_heads, database_key_index);
        match try_complete_cycle_head(
            active_query,
            &mut self.query.execution.claim_guard,
            self.cycle_heads,
            &self.last_provisional_memo.header.revisions,
            self.outer_cycle,
            self.query.iteration,
            self.cycle_iteration,
            value_converged,
        ) {
            CycleResolution::Return(completed_query) => {
                self.query.complete(new_value, completed_query)
            }
            CycleResolution::Repeat {
                completed,
                cycle_heads,
                next_iteration,
            } => ExecutionStep::Prepare(CompletionPreparation {
                value: new_value,
                completed,
                disposition: CompletionDisposition::Repeat {
                    query: self.query,
                    cycle_heads,
                    next_iteration,
                },
            }),
            CycleResolution::Finalize {
                completed,
                cycle_heads,
                iteration,
            } => {
                // Keep Local masked until the accepted cycle is published and its claim retires.
                // Unexpected failures during preparation still poison the provisional result.
                drop(self.query.last_stale_tracked_ids);
                ExecutionStep::Prepare(CompletionPreparation {
                    value: new_value,
                    completed,
                    disposition: CompletionDisposition::Finalize {
                        poison_guard: self.query.poison_guard,
                        cancellation_guard: self.query.cancellation_guard,
                        execution: self.query.execution,
                        cycle_heads,
                        iteration,
                    },
                })
            }
        }
    }
}

enum InitialContinuation<'db, C: Configuration> {
    Participant(CycleParticipant<'db, C>),
    Head(CycleHead<'db, C>),
}

impl<'db, C: Configuration> InitialContinuation<'db, C> {
    fn execution(&self) -> &QueryExecution<'db, C> {
        match self {
            Self::Participant(participant) => &participant.query.execution,
            Self::Head(head) => &head.query.execution,
        }
    }
}

struct CycleInitial<'db, C: Configuration> {
    // Unwinding retires the value while its query frame and claim are still owned.
    computed_value: C::Output<'db>,
    active_query: ActiveQueryGuard<'db>,
    continuation: InitialContinuation<'db, C>,
}

impl<'db, C: Configuration> CycleInitial<'db, C> {
    fn execute(self) -> ExecutionStep<'db, C> {
        let execution = self.continuation.execution();
        let id = self.active_query.database_key_index.key_index();
        let new_value = C::cycle_initial(
            execution.db,
            id,
            C::id_to_input(execution.claim_guard.zalsa(), id),
        );
        self.resume(new_value)
    }

    fn resume(self, new_value: C::Output<'db>) -> ExecutionStep<'db, C> {
        ExecutionStep::RetireInitial(InitialReplacement {
            old_value: Some(self.computed_value),
            replacement: new_value,
            active_query: self.active_query,
            continuation: self.continuation,
        })
    }
}

struct InitialReplacement<'db, C: Configuration> {
    old_value: Option<C::Output<'db>>,
    replacement: C::Output<'db>,
    active_query: ActiveQueryGuard<'db>,
    continuation: InitialContinuation<'db, C>,
}

impl<'db, C: Configuration> InitialReplacement<'db, C> {
    fn retire(&mut self) {
        // Taking first prevents a panicking destructor from being called a second time.
        drop(self.old_value.take());
    }

    fn execute(mut self) -> ExecutionStep<'db, C> {
        self.retire();
        self.resume()
    }

    fn resume(self) -> ExecutionStep<'db, C> {
        match self.continuation {
            InitialContinuation::Participant(participant) => {
                participant.resume(self.active_query, self.replacement)
            }
            InitialContinuation::Head(head) => RecoveryContinuation {
                active_query: self.active_query,
                head,
            }
            .resume(self.replacement),
        }
    }
}

struct RecoveryContinuation<'db, C: Configuration> {
    active_query: ActiveQueryGuard<'db>,
    head: CycleHead<'db, C>,
}

struct CycleRecovery<'db, C: Configuration> {
    new_value: C::Output<'db>,
    continuation: RecoveryContinuation<'db, C>,
}

impl<'db, C: Configuration> CycleRecovery<'db, C> {
    fn execute(self) -> ExecutionStep<'db, C> {
        let Self {
            continuation,
            new_value,
        } = self;
        let execution = &continuation.head.query.execution;
        let id = continuation.active_query.database_key_index.key_index();
        let cycle = Cycle {
            head_ids: continuation.head.cycle_heads.ids(),
            id,
            iteration: continuation.head.cycle_iteration.iteration_as_u32(),
        };
        // Input conversion can clone user values. Keep the computed operand owned here until
        // conversion succeeds, before handing either argument to the recovery function.
        let input = C::id_to_input(execution.claim_guard.zalsa(), id);
        // We are in a cycle that hasn't converged; ask the user's
        // cycle-recovery function what to do (it may return the same value or a different one):
        let new_value = C::recover_from_cycle(
            execution.db,
            &cycle,
            continuation.head.last_provisional_value,
            new_value,
            input,
        );
        continuation.resume(new_value)
    }
}

impl<'db, C: Configuration> RecoveryContinuation<'db, C> {
    fn resume(self, new_value: C::Output<'db>) -> ExecutionStep<'db, C> {
        if self.active_query.attempt_incomplete() {
            return self
                .head
                .query
                .finish_incomplete(self.active_query, new_value);
        }
        ExecutionStep::CompareCycle(CycleComparison {
            new_value,
            continuation: self,
        })
    }
}

struct CycleComparison<'db, C: Configuration> {
    new_value: C::Output<'db>,
    continuation: RecoveryContinuation<'db, C>,
}

impl<'db, C: Configuration> CycleComparison<'db, C> {
    fn comparison_values(&self) -> Option<(&C::Output<'db>, &C::Output<'db>)> {
        (C::CYCLE_STRATEGY != CycleRecoveryStrategy::FallbackImmediate).then_some((
            &self.new_value,
            self.continuation.head.last_provisional_value,
        ))
    }

    fn compare(&self) -> bool {
        self.comparison_values()
            .is_none_or(|(left, right)| C::values_equal(left, right))
    }

    fn execute(self) -> ExecutionStep<'db, C> {
        let value_converged = self.compare();
        self.resume(value_converged)
    }

    fn resume(self, value_converged: bool) -> ExecutionStep<'db, C> {
        if self.continuation.active_query.attempt_incomplete() {
            return self
                .continuation
                .head
                .query
                .finish_incomplete(self.continuation.active_query, self.new_value);
        }
        self.continuation.head.compared(
            self.continuation.active_query,
            self.new_value,
            value_converged,
        )
    }
}

struct PreviousIteration {
    iteration: IterationStamp,
    reuse_as_provisional: bool,
}

enum QueryExecutionOutcome<'db> {
    Completed(CompletedQuery),
    Participant {
        active_query: ActiveQueryGuard<'db>,
        cycle_heads: CycleHeads,
        outer_cycle: DatabaseKeyIndex,
    },
    CycleHead {
        active_query: ActiveQueryGuard<'db>,
        cycle_heads: CycleHeads,
        outer_cycle: Option<DatabaseKeyIndex>,
        cycle_iteration: IterationStamp,
    },
}

impl MemoHeader {
    fn previous_iteration(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
        cancellation_count: u8,
        has_value: bool,
    ) -> Option<PreviousIteration> {
        if self.revisions.iteration().cancellation_count() != cancellation_count {
            #[cfg(all(test, not(feature = "shuttle")))]
            transfer_trace::record(
                TransferEvent::new(Kind::Previous)
                    .key(database_key_index)
                    .memo(Some(self.transfer_test_snapshot(has_value)))
                    .decision(false),
            );
            return None;
        }

        // The `DependencyGraph` locking propagates panics when another thread is blocked on a panicking query.
        // However, the locking doesn't handle the case where a thread fetches the result of a panicking
        // cycle head query **after** all locks were released. That's what we do here.
        // We could consider re-executing the entire cycle but:
        // a) It's tricky to ensure that all queries participating in the cycle will re-execute
        //    (we can't rely on `iteration` being updated for nested cycles because the nested cycles may have completed successfully).
        // b) It's guaranteed that this query will panic again anyway.
        // That's why we simply propagate the panic here. It simplifies our lives and it also avoids duplicate panic messages.
        if !has_value {
            tracing::warn!(
                "Propagating panic for cycle head that panicked in an earlier execution in that revision"
            );
            Cancelled::PropagatedPanic.throw();
        }

        if !self.can_seed_attempt(zalsa) {
            #[cfg(all(test, not(feature = "shuttle")))]
            transfer_trace::record(
                TransferEvent::new(Kind::Previous)
                    .key(database_key_index)
                    .memo(Some(self.transfer_test_snapshot(has_value)))
                    .decision(false),
            );
            return None;
        }

        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            TransferEvent::new(Kind::Previous)
                .key(database_key_index)
                .memo(Some(self.transfer_test_snapshot(has_value)))
                .decision(true),
        );
        Some(PreviousIteration {
            iteration: self.revisions.iteration(),
            // Only use the last provisional memo if it was a cycle head in the last iteration. This is to
            // force at least two executions.
            reuse_as_provisional: self.cycle_heads().contains(&database_key_index),
        })
    }

    fn seed_active_query(&self, zalsa: &Zalsa, active_query: &ActiveQueryGuard<'_>) -> bool {
        if !self.can_seed_attempt(zalsa) {
            #[cfg(all(test, not(feature = "shuttle")))]
            {
                let mut event = TransferEvent::new(Kind::SeedActive)
                    .key(active_query.database_key_index)
                    .decision(false);
                event.identity = std::ptr::from_ref(self).addr();
                event.support = self
                    .revisions
                    .attempt_support()
                    .map(transfer_trace::support_snapshot);
                transfer_trace::record(event);
            }
            // QueryBody retains this header's output ownership without reusing its approximation.
            return false;
        }
        // Copy over all inputs and outputs from a previous iteration.
        // This is necessary to:
        // * ensure that tracked struct created during the previous iteration
        //   (and are owned by the query) are alive even if the query in this iteration no longer creates them.
        // * ensure the final returned memo depends on all inputs from all iterations.
        if self.may_be_provisional() && self.verified_at.load() == zalsa.current_revision() {
            active_query.seed_iteration(zalsa, &self.revisions);
            #[cfg(all(test, not(feature = "shuttle")))]
            {
                let mut event = TransferEvent::new(Kind::SeedActive)
                    .key(active_query.database_key_index)
                    .decision(true);
                event.identity = std::ptr::from_ref(self).addr();
                event.support = self
                    .revisions
                    .attempt_support()
                    .map(transfer_trace::support_snapshot);
                transfer_trace::record(event);
            }
        } else {
            // If we already executed this query once, then use the tracked-struct ids from the
            // previous execution as the starting point for the new one.
            active_query.seed_tracked_struct_ids(self.revisions.tracked_struct_ids());
        }
        true
    }
}

fn finish_incomplete_query(
    active_query: ActiveQueryGuard<'_>,
    claim_guard: &mut ClaimGuard<'_>,
    iteration: IterationStamp,
) -> CompletedQuery {
    active_query.finish_attempt_incomplete();
    claim_guard.set_release_mode(ReleaseMode::Default);
    active_query.pop(iteration)
}

fn try_complete_query<'db>(
    zalsa: &Zalsa,
    active_query: ActiveQueryGuard<'db>,
    claim_guard: &mut ClaimGuard<'db>,
    iteration: IterationStamp,
    cycle_heads: CycleHeads,
    max_iteration: IterationStamp,
    depends_on_self: bool,
) -> QueryExecutionOutcome<'db> {
    let database_key_index = active_query.database_key_index;

    // If there are no cycle heads, break out of the loop.
    if cycle_heads.is_empty() {
        // There's no cycle iteration state to preserve.
        let iteration = if iteration.is_initial_iteration() {
            IterationStamp::default()
        } else {
            iteration.increment_iteration().unwrap_or_else(|| {
                tracing::warn!("{database_key_index:?}: execute: too many cycle iterations");
                panic!("{database_key_index:?}: execute: too many cycle iterations")
            })
        };

        return QueryExecutionOutcome::Completed(active_query.pop(iteration));
    }

    let outer_cycle = outer_cycle(
        zalsa,
        claim_guard.zalsa_local(),
        &cycle_heads,
        database_key_index,
    );

    // Did the new result we got depend on our own provisional value, in a cycle?
    // If not, return because this query is not a cycle head.
    if !depends_on_self {
        let Some(outer_cycle) = outer_cycle else {
            panic!(
                "cycle participant with non-empty cycle heads and that doesn't depend on itself must have an outer cycle responsible to finalize the query later (query: {database_key_index:?}, cycle heads: {cycle_heads:?})."
            );
        };

        return QueryExecutionOutcome::Participant {
            active_query,
            cycle_heads,
            outer_cycle,
        };
    }

    // If this is the outermost cycle, use the maximum iteration count of all cycles.
    // This is important for when later iterations introduce new cycle heads (that then
    // become the outermost cycle). We want to ensure that the iteration count keeps increasing
    // for all queries or they won't be re-executed because `validate_same_iteration` would
    // pass when we go from 1 -> 0 and then increment by 1 to 1).
    let cycle_iteration = if outer_cycle.is_none() {
        max_iteration
    } else {
        // Otherwise keep the iteration count because outer cycles
        // already have a cycle head with this exact iteration count (and we don't allow
        // heads from different iterations).
        iteration
    };

    QueryExecutionOutcome::CycleHead {
        active_query,
        cycle_heads,
        outer_cycle,
        cycle_iteration,
    }
}

#[must_use]
struct DisableLocalCancellationGuard<'a> {
    zalsa_local: &'a ZalsaLocal,
    was_disabled: bool,
}

impl<'a> DisableLocalCancellationGuard<'a> {
    fn new(zalsa_local: &'a ZalsaLocal) -> Self {
        Self {
            zalsa_local,
            was_disabled: zalsa_local.set_cancellation_disabled(true),
        }
    }
}

impl Drop for DisableLocalCancellationGuard<'_> {
    fn drop(&mut self) {
        self.zalsa_local
            .set_cancellation_disabled(self.was_disabled);
    }
}

/// Replaces any inserted memo with a fixpoint initial memo without a value if the current thread panics.
///
/// A regular query doesn't insert any memo if it panics and the query
/// simply gets re-executed if any later called query depends on the panicked query (and will panic again unless the query isn't deterministic).
///
/// Unfortunately, this isn't the case for cycle heads because Salsa first inserts the fixpoint initial memo and later inserts
/// provisional memos for every iteration. Detecting whether a query has previously panicked
/// in `fetch` (e.g., `validate_same_iteration`) and requires re-execution is probably possible but not very straightforward
/// and it's easy to get it wrong, which results in infinite loops where `Memo::provisional_retry` keeps retrying to get the latest `Memo`
/// but `fetch` doesn't re-execute the query for reasons.
///
/// Specifically, a Memo can linger after a panic, which is then incorrectly returned
/// by `fetch_cold_cycle` because it passes the `shallow_verified_memo` check instead of inserting
/// a new fix point initial value if that happens.
///
/// We could insert a fixpoint initial value here, but it seems unnecessary.
struct PoisonProvisionalIfPanicking<'a, C: Configuration> {
    ingredient: &'a IngredientImpl<C>,
    zalsa: &'a Zalsa,
    id: Id,
    memo_ingredient_index: MemoIngredientIndex,
}

impl<'a, C: Configuration> PoisonProvisionalIfPanicking<'a, C> {
    fn new(
        ingredient: &'a IngredientImpl<C>,
        zalsa: &'a Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Self {
        Self {
            ingredient,
            zalsa,
            id,
            memo_ingredient_index,
        }
    }
}

fn poison_provisional_memo<C: Configuration>(
    ingredient: &IngredientImpl<C>,
    zalsa: &Zalsa,
    id: Id,
    memo_ingredient_index: MemoIngredientIndex,
) {
    let revisions = QueryRevisions::fixpoint_initial(
        zalsa,
        ingredient.database_key_index(id),
        IterationStamp::initial(zalsa.runtime().cancellation_count()),
    );
    let memo = Memo::new(None, zalsa.current_revision(), revisions);
    let _inserted = ingredient.insert_memo(zalsa, id, memo, memo_ingredient_index);
    #[cfg(all(test, not(feature = "shuttle")))]
    transfer_trace::record(
        TransferEvent::new(Kind::Poisoned)
            .key(ingredient.database_key_index(id))
            .memo(Some(_inserted.transfer_test_snapshot())),
    );
}

impl<C: Configuration> Drop for PoisonProvisionalIfPanicking<'_, C> {
    fn drop(&mut self) {
        if thread::panicking() {
            poison_provisional_memo(
                self.ingredient,
                self.zalsa,
                self.id,
                self.memo_ingredient_index,
            );
        }
    }
}

/// Returns the key of any potential outer cycle head or `None` if there is no outer cycle.
///
/// That is, any query that's currently blocked on the result computed by this query (claiming it results in a cycle).
fn outer_cycle(
    zalsa: &Zalsa,
    zalsa_local: &ZalsaLocal,
    cycle_heads: &CycleHeads,
    current_key: DatabaseKeyIndex,
) -> Option<DatabaseKeyIndex> {
    // First, look for the outer most cycle head on the same thread.
    // Using the outer most over the inner most should reduce the need
    // for transitive transfers.
    // SAFETY: We don't call into with_query_stack recursively
    if let Some(same_thread) = unsafe {
        zalsa_local.with_query_stack_unchecked(|stack| {
            stack
                .iter()
                .find(|active_query| {
                    active_query.database_key_index != current_key
                        && cycle_heads.contains(&active_query.database_key_index)
                })
                .map(|active_query| active_query.database_key_index)
        })
    } {
        return Some(same_thread);
    }

    // Check for any outer cycle head running on a different thread.
    cycle_heads
        .iter_not_eq(current_key)
        .rfind(|head| {
            let function = zalsa
                .lookup_ingredient(head.database_key_index.ingredient_index())
                .as_function()
                .expect("cycle heads must be function ingredients");

            matches!(
                function.sync_table().peek_claim(
                    zalsa,
                    head.database_key_index.key_index(),
                    Reentrancy::Deny,
                ),
                ClaimResult::Cycle { inner: false }
            )
        })
        .map(|head| head.database_key_index)
}

/// The active frame and computed value stay owned while transitive cycle heads are admitted.
struct ExpandHeads<'db, C: Configuration> {
    new_value: C::Output<'db>,
    active_query: ActiveQueryGuard<'db>,
    query: IteratingQuery<'db, C>,
    traversal: HeadTraversal<'db>,
}

impl<'db, C: Configuration> ExpandHeads<'db, C> {
    fn work(&self) -> Option<HeadWork<'db>> {
        self.traversal
            .work(self.query.execution.claim_guard.zalsa())
    }

    fn advance(mut self, work: Option<HeadWork<'db>>) -> ExecutionStep<'db, C> {
        let zalsa = self.query.execution.claim_guard.zalsa();
        if self.active_query.attempt_incomplete() {
            return self
                .query
                .finish_incomplete(self.active_query, self.new_value);
        }
        if let Some(work) = work {
            self.traversal.advance(zalsa, work);
            return ExecutionStep::ExpandHeads(self);
        }
        if !self.traversal.is_current(zalsa) {
            self.traversal.restart();
            return ExecutionStep::ExpandHeads(self);
        }
        let (heads, maximum, depends_on_self) = self.traversal.finish();
        self.query.after_heads(
            self.active_query,
            self.new_value,
            heads,
            maximum,
            depends_on_self,
        )
    }

    fn execute(self) -> ExecutionStep<'db, C> {
        let work = self.work();
        assert!(
            work.is_some() || self.traversal.complete(),
            "cycle head traversal work overflow"
        );
        self.advance(work)
    }
}

// Called when completing the query of a cycle head participating
// in an outer cycle head (which doesn't depend on itself).
fn complete_cycle_participant(
    active_query: ActiveQueryGuard,
    claim_guard: &mut ClaimGuard,
    cycle_heads: CycleHeads,
    outer_cycle: DatabaseKeyIndex,
    iteration: IterationStamp,
) -> CompletedQuery {
    // For as long as this query participates in any cycle, don't release its lock, instead
    // transfer it to the outermost cycle head. This prevents any other thread
    // from claiming this query (all cycle heads are potential entry points to the same cycle),
    // which would result in them competing for the same locks (we want the locks to converge to a single cycle head).
    claim_guard.set_release_mode(ReleaseMode::TransferTo(outer_cycle));
    let zalsa = claim_guard.zalsa();

    let database_key_index = active_query.database_key_index;
    let iteration = iteration.increment_iteration().unwrap_or_else(|| {
        tracing::warn!("{database_key_index:?}: execute: too many cycle iterations");
        panic!("{database_key_index:?}: execute: too many cycle iterations")
    });

    let mut completed_query = complete_cycle_query(zalsa, active_query, iteration);

    *completed_query.revisions.verified_final.get_mut() = false;
    completed_query
        .revisions
        .set_cycle_heads(cycle_heads, iteration);

    completed_query
}

enum CycleResolution {
    Return(CompletedQuery),
    Repeat {
        completed: CompletedQuery,
        cycle_heads: CycleHeads,
        next_iteration: IterationStamp,
    },
    Finalize {
        completed: CompletedQuery,
        cycle_heads: CycleHeads,
        iteration: IterationStamp,
    },
}

/// Resolves convergence without publishing finality or changing stored iteration counts.
#[allow(clippy::too_many_arguments)]
fn try_complete_cycle_head(
    active_query: ActiveQueryGuard,
    claim_guard: &mut ClaimGuard,
    cycle_heads: CycleHeads,
    last_provisional_revisions: &QueryRevisions,
    outer_cycle: Option<DatabaseKeyIndex>,
    iteration: IterationStamp,
    max_iteration: IterationStamp,
    value_converged: bool,
) -> CycleResolution {
    let me = active_query.database_key_index;
    let zalsa = claim_guard.zalsa();

    let mut completed_query = complete_cycle_query(zalsa, active_query, iteration);
    assert!(
        !completed_query.revisions.has_incomplete_attempt(),
        "incomplete cycle reached convergence"
    );

    // It's important to force a re-execution of the cycle if `changed_at` or `durability` has changed
    // to ensure the reduced durability and changed propagates to all queries depending on this head.
    let metadata_converged = last_provisional_revisions.durability
        == completed_query.revisions.durability
        && last_provisional_revisions.changed_at == completed_query.revisions.changed_at
        && last_provisional_revisions.is_derived_untracked()
            == completed_query.revisions.is_derived_untracked();

    let this_converged = value_converged && metadata_converged;

    if let Some(outer_cycle) = outer_cycle {
        tracing::info!(
            "Detected nested cycle {me:?}, iterate it as part of the outer cycle {outer_cycle:?}"
        );

        completed_query
            .revisions
            .set_cycle_heads(cycle_heads, max_iteration);
        // Store whether this cycle has converged, so that the outer cycle can check it.
        completed_query
            .revisions
            .set_cycle_converged(this_converged);
        *completed_query.revisions.verified_final.get_mut() = false;

        // Transfer ownership of this query to the outer cycle, so that it can claim it
        // and other threads don't compete for the same lock.
        claim_guard.set_release_mode(ReleaseMode::TransferTo(outer_cycle));

        return CycleResolution::Return(completed_query);
    }

    // This is the outermost cycle, drive the cycle forward:
    // ..test if all inner cycles have converged as well.
    let converged = this_converged
        && cycle_heads.iter_not_eq(me).all(|head| {
            let database_key_index = head.database_key_index;
            let function = zalsa
                .lookup_ingredient(database_key_index.ingredient_index())
                .as_function()
                .expect("cycle heads must be function ingredients");

            let converged = function
                .memo(zalsa, database_key_index.key_index())
                .is_none_or(|memo| {
                    !memo.header().has_incomplete_attempt() && memo.header().cycle_converged()
                });

            if !converged {
                tracing::debug!("inner cycle {database_key_index:?} has not converged",);
            }

            converged
        });

    if converged {
        tracing::debug!(
            "{me:?}: execute: fixpoint iteration has a final value after {max_iteration:?} iterations",
            max_iteration = max_iteration.iteration()
        );

        *completed_query.revisions.verified_final.get_mut() = true;
        return CycleResolution::Finalize {
            completed: completed_query,
            cycle_heads,
            iteration: max_iteration,
        };
    }

    // The fixpoint iteration hasn't converged. Iterate again...
    let iteration = max_iteration.increment_iteration().unwrap_or_else(|| {
        tracing::warn!("{me:?}: execute: too many cycle iterations");
        panic!("{me:?}: execute: too many cycle iterations")
    });

    CycleResolution::Repeat {
        completed: completed_query,
        cycle_heads,
        next_iteration: iteration,
    }
}

fn assert_no_new_cycle_heads(
    cycle_heads: &CycleHeads,
    new_cycle_heads: CycleHeads,
    me: DatabaseKeyIndex,
) {
    for head in new_cycle_heads {
        if !cycle_heads.contains(&head.database_key_index) {
            panic!(
                "Cycle recovery function for {me:?} introduced a cycle, depending on {:?}. This is not allowed.",
                head.database_key_index
            );
        }
    }
}

thread_local! {
    /// Pool the `seen` and `flattened` sets for reuse on the same thread.
    ///
    /// Benchmarks showed that repeatedly allocating and regrowing those sets is expensive.
    static FLATTEN_MAPS: std::cell::Cell<Option<(FxIndexSet<QueryEdge>, FxHashSet<DatabaseKeyIndex>)>> = const { std::cell::Cell::new(None) };
}

fn complete_cycle_query(
    zalsa: &Zalsa,
    active_query: ActiveQueryGuard<'_>,
    iteration: IterationStamp,
) -> CompletedQuery {
    let (mut flattened, mut seen) = FLATTEN_MAPS.take().unwrap_or_default();

    debug_assert!(flattened.is_empty());
    debug_assert!(seen.is_empty());

    let detached_query = active_query.detach();
    flattened.reserve(detached_query.input_outputs().len());
    flatten_cycle_dependencies(
        zalsa,
        detached_query.input_outputs(),
        &mut flattened,
        &mut seen,
    );

    seen.clear();
    let completion = detached_query.pop_completion(iteration, true);
    let completed_query = completion.finish(flattened.drain(..));
    #[cfg(feature = "accumulator")]
    assert!(
        completed_query
            .revisions
            .accumulated_inputs
            .load()
            .is_empty(),
        "Fixpoint iteration doesn't support accumulated values because it doesn't preserve the original query dependency tree."
    );
    FLATTEN_MAPS.set(Some((flattened, seen)));
    completed_query
}

/// Flattens the dependencies of `head` so that `head`'s origin only depends on finalized queries,
/// or salsa structs (input, tracked, interned).
fn flatten_cycle_dependencies(
    zalsa: &Zalsa,
    direct_input_outputs: &FxIndexSet<QueryEdge>,
    flattened: &mut FxIndexSet<QueryEdge>,
    seen: &mut FxHashSet<DatabaseKeyIndex>,
) {
    // Don't insert the key of `head` here. This is important to ensure that we copy over the
    // dependencies from this memo in the previous iteration.
    // e.g. if we have `a2 -> b2 -> a1`, we need to copy over `a`'s dependencies from iteration 1.
    for edge in direct_input_outputs.iter().copied() {
        match edge.kind() {
            QueryEdgeKind::Input => {
                let input = edge.key();
                let ingredient = zalsa.lookup_ingredient(input.ingredient_index());
                ingredient.flatten_cycle_head_dependencies(
                    zalsa,
                    input.key_index(),
                    flattened,
                    seen,
                );
            }

            QueryEdgeKind::Output => {
                // Unlike `ingredient.collect_flattened_cycle_inputs`, carry over outputs
                // created by the query head because those are owned by this query.
                flattened.insert(edge);
            }
        }
    }
}
