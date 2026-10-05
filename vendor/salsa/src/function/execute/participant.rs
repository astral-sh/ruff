use super::{DisableLocalCancellationGuard, PreviousMemo, QueryExecution, poison_provisional_memo};
use crate::attempt_probe::MemoReuse;
use crate::function::memo::{ErasedMemo, Memo, MemoHeader};
use crate::function::{ClaimGuard, Configuration, IngredientImpl};

use crate::DatabaseKeyIndex;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Event as TransferEvent, Kind,
};
use crate::runtime::{RetirementQuote, TransferFailure};
use crate::sync::thread;
use crate::zalsa_local::ZalsaLocal;

use crate::cycle::{CycleHeads, IterationStamp, ProvisionalStatus};
use crate::zalsa::Zalsa;

#[cfg(all(test, not(feature = "shuttle")))]
mod event_tests;
#[cfg(all(test, not(feature = "shuttle")))]
mod tests;

/// The read recipient is captured before the child claim and stays lexically owned by the caller.
pub(in crate::function) struct ReadRecipient {
    key: DatabaseKeyIndex,
    depth: usize,
}

pub(in crate::function) enum Consumer {
    Read(ReadRecipient),
    Validation,
    FinalValue,
    FinalMetadata,
}

impl Consumer {
    pub(in crate::function) fn capture(local: &ZalsaLocal) -> Self {
        local
            .try_with_query_stack(|stack| match stack.last() {
                Some(query) => Self::Read(ReadRecipient {
                    key: query.database_key_index,
                    depth: stack.len(),
                }),
                None => Self::FinalValue,
            })
            .unwrap_or(Self::FinalValue)
    }

    pub(in crate::function) fn is_current(&self, local: &ZalsaLocal) -> bool {
        match self {
            Self::Read(recipient) => {
                local.try_with_query_stack(|stack| {
                    stack.len() == recipient.depth
                        && stack
                            .last()
                            .is_some_and(|query| query.database_key_index == recipient.key)
                }) == Some(true)
            }
            Self::FinalValue | Self::Validation | Self::FinalMetadata => true,
        }
    }

    pub(in crate::function) fn for_child(&self, local: &ZalsaLocal) -> Self {
        assert!(
            self.is_current(local),
            "provisional query lost its enclosing read recipient"
        );
        match self {
            Self::Read(recipient) => Self::Read(ReadRecipient {
                key: recipient.key,
                depth: recipient.depth,
            }),
            Self::Validation => Self::Validation,
            Self::FinalValue => Self::FinalValue,
            Self::FinalMetadata => Self::FinalMetadata,
        }
    }

    fn may_continue(&self) -> bool {
        matches!(self, Self::Read(_) | Self::Validation)
    }

    pub(in crate::function) fn require_seed_recipient(&self, local: &ZalsaLocal) {
        if matches!(self, Self::FinalMetadata) {
            panic!(
                "Fixpoint iteration doesn't support accumulated values: metadata refresh requires a final result"
            );
        }
        assert!(
            matches!(self, Self::Read(_)) && self.is_current(local),
            "dependency graph cycle has no enclosing query to receive a provisional value"
        );
    }
}

/// A published approximation remains claim-owned until transfer or a checked internal return.
/// Fields restore Local before releasing the claim during unwinding.
pub(in crate::function) struct Participant<'db, C: Configuration> {
    cancellation_guard: Option<DisableLocalCancellationGuard<'db>>,
    execution: Option<QueryExecution<'db, C>>,
    memo: &'db Memo<C>,
    original_iteration: IterationStamp,
    traversal: HeadTraversal<'db>,
    cached: bool,
    poisoned: bool,
}

pub(in crate::function) enum ParticipantWork<'db> {
    Expand(HeadWork<'db>, usize),
    Transfer(RetirementQuote),
}

impl ParticipantWork<'_> {
    pub(in crate::function) fn units(&self) -> usize {
        match self {
            Self::Expand(work, check) => work.units + check,
            Self::Transfer(quote) => quote.units,
        }
    }
    pub(in crate::function) fn bytes(&self) -> usize {
        match self {
            Self::Expand(work, _) => work.bytes,
            Self::Transfer(quote) => quote.scratch_bytes,
        }
    }
}

pub(in crate::function) enum ParticipantProgress<'db, C: Configuration> {
    Pending(Participant<'db, C>),
    Complete(Option<&'db Memo<C>>),
    Execute(QueryExecution<'db, C>),
}

impl<'db, C: Configuration> Participant<'db, C> {
    pub(super) fn published(
        execution: QueryExecution<'db, C>,
        memo: &'db Memo<C>,
        cancellation_guard: Option<DisableLocalCancellationGuard<'db>>,
    ) -> Self {
        Self::new(
            execution,
            memo,
            cancellation_guard,
            memo.header.revisions.cycle_heads().clone(),
            false,
        )
    }

    fn new(
        execution: QueryExecution<'db, C>,
        memo: &'db Memo<C>,
        cancellation_guard: Option<DisableLocalCancellationGuard<'db>>,
        heads: CycleHeads,
        cached: bool,
    ) -> Self {
        let cancellation_guard = cancellation_guard.or_else(|| {
            Some(DisableLocalCancellationGuard::new(
                execution.claim_guard.zalsa_local(),
            ))
        });
        let original_iteration = memo.header.revisions.iteration();
        let traversal = HeadTraversal::new(
            heads,
            execution.claim_guard.database_key_index(),
            original_iteration,
        );
        Self {
            cancellation_guard,
            execution: Some(execution),
            memo,
            original_iteration,
            traversal,
            cached,
            poisoned: false,
        }
    }

    pub(in crate::function) fn cached(
        ingredient: &'db IngredientImpl<C>,
        db: &'db C::DbView,
        claim: ClaimGuard<'db>,
        memo: &'db Memo<C>,
        consumer: Consumer,
        heads: CycleHeads,
    ) -> Self {
        let memo_ingredient_index =
            ingredient.memo_ingredient_index(claim.zalsa(), claim.database_key_index().key_index());
        let execution = QueryExecution {
            ingredient,
            db,
            previous: Some(PreviousMemo::Semantic(memo)),
            memo_ingredient_index,
            claim_guard: claim,
            start_incomplete: false,
            consumer,
        };
        Self::new(execution, memo, None, heads, true)
    }

    #[cfg(test)]
    pub(in crate::function) fn key(&self) -> Option<DatabaseKeyIndex> {
        self.execution
            .as_ref()
            .map(|execution| execution.claim_guard.database_key_index())
    }

    pub(in crate::function) fn work(&self) -> Option<ParticipantWork<'db>> {
        let execution = self.execution.as_ref()?;
        let check = self
            .traversal
            .root
            .storage_len()
            .checked_mul(2)?
            .checked_add(8)?;
        if !self.traversal.complete() {
            let work = self.traversal.work(execution.claim_guard.zalsa())?;
            work.units.checked_add(check)?;
            Some(ParticipantWork::Expand(work, check))
        } else {
            let mut quote = execution
                .claim_guard
                .zalsa()
                .runtime()
                .retirement_quote(self.traversal.heads.len())?;
            let depth = execution
                .claim_guard
                .zalsa_local()
                .try_with_query_stack(|stack| stack.len())
                .unwrap_or(0);
            quote.units = quote
                .units
                .checked_add(check)?
                .checked_add(self.traversal.check_work()?)?
                .checked_add(depth.checked_mul(self.traversal.heads.len())?)?;
            Some(ParticipantWork::Transfer(quote))
        }
    }

    pub(in crate::function) fn advance(
        mut self,
        work: ParticipantWork<'db>,
    ) -> ParticipantProgress<'db, C> {
        let Some(execution) = self.execution.as_ref() else {
            return ParticipantProgress::Pending(self);
        };
        assert!(
            execution
                .consumer
                .is_current(execution.claim_guard.zalsa_local()),
            "participant lost its read recipient"
        );
        let zalsa = execution.claim_guard.zalsa();
        let current = execution.ingredient.get_memo_from_table_for(
            zalsa,
            execution.claim_guard.database_key_index().key_index(),
            execution.memo_ingredient_index,
        );
        if let Some(current) = current {
            match current.header.attempt_reuse(zalsa) {
                MemoReuse::Incomplete => return self.complete(current),
                MemoReuse::Stale => return self.reexecute(Some(current)),
                MemoReuse::Ordinary => {}
            }
        }
        if let Some(current) = current
            && !current.header.may_be_provisional()
            && current.header.verified_at.load() == zalsa.current_revision()
            && current.header.attempt_reuse(zalsa) == MemoReuse::Ordinary
        {
            return self.complete(current);
        }
        if current.is_none_or(|current| !std::ptr::eq(current, self.memo))
            || self.memo.header.revisions.iteration() != self.original_iteration
            || self.memo.header.revisions.cycle_heads().storage_len()
                > self.traversal.root.storage_len()
            || !self
                .memo
                .header
                .revisions
                .cycle_heads()
                .into_iter()
                .map(|head| {
                    #[cfg(all(test, not(feature = "shuttle")))]
                    tests::record_evidence_scan();
                    (head.database_key_index, head.iteration.load())
                })
                .eq((&self.traversal.root)
                    .into_iter()
                    .map(|head| (head.database_key_index, head.iteration.load())))
        {
            return self.reexecute(current);
        }
        match work {
            ParticipantWork::Expand(work, _) => {
                self.traversal.advance(zalsa, work);
                ParticipantProgress::Pending(self)
            }
            ParticipantWork::Transfer(quote) => {
                if !self.traversal.is_current(zalsa) {
                    self.traversal.restart();
                    return ParticipantProgress::Pending(self);
                }
                self.retire(quote)
            }
        }
    }

    fn complete(mut self, memo: &'db Memo<C>) -> ParticipantProgress<'db, C> {
        let Some(mut execution) = self.execution.take() else {
            return ParticipantProgress::Pending(self);
        };
        #[cfg(all(test, not(feature = "shuttle")))]
        let (key, serial) = (
            execution.claim_guard.database_key_index(),
            execution.claim_guard.test_serial(),
        );
        execution
            .claim_guard
            .set_release_mode(super::ReleaseMode::Default);
        let refetch = execution.claim_guard.drop();
        drop(self.cancellation_guard.take());
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            TransferEvent::new(Kind::Refetch)
                .key(key)
                .serial(serial)
                .decision(refetch),
        );
        ParticipantProgress::Complete(if refetch { None } else { Some(memo) })
    }

    fn reexecute(mut self, memo: Option<&'db Memo<C>>) -> ParticipantProgress<'db, C> {
        let Some(mut execution) = self.execution.take() else {
            return ParticipantProgress::Pending(self);
        };
        execution.previous = memo.map(PreviousMemo::Semantic);
        drop(self.cancellation_guard.take());
        ParticipantProgress::Execute(execution)
    }

    fn retire(mut self, quote: RetirementQuote) -> ParticipantProgress<'db, C> {
        let Some(mut execution) = self.execution.take() else {
            return ParticipantProgress::Pending(self);
        };
        let zalsa = execution.claim_guard.zalsa();
        let local = execution.claim_guard.zalsa_local();
        let key = execution.claim_guard.database_key_index();
        #[cfg(all(test, not(feature = "shuttle")))]
        let serial = execution.claim_guard.test_serial();
        if self.memo.header.attempt_reuse(zalsa) == MemoReuse::Incomplete {
            execution
                .claim_guard
                .set_release_mode(super::ReleaseMode::Default);
            let refetch = execution.claim_guard.drop();
            drop(self.cancellation_guard.take());
            #[cfg(all(test, not(feature = "shuttle")))]
            transfer_trace::record(
                TransferEvent::new(Kind::Refetch)
                    .key(key)
                    .serial(serial)
                    .decision(refetch),
            );
            return ParticipantProgress::Complete(if refetch { None } else { Some(self.memo) });
        }
        let mut compatible = self.memo.header.attempt_reuse(zalsa) == MemoReuse::Ordinary;
        if self.cached {
            compatible = self.memo.header.provisional_epoch_is_current(zalsa);
        }
        if self.cached && compatible {
            for head in &self.traversal.root {
                if head.database_key_index != key {
                    match execution
                        .claim_guard
                        .check_participant_head(head.database_key_index, &quote)
                    {
                        Ok(()) => {}
                        Err(TransferFailure::Requote) => {
                            self.execution = Some(execution);
                            return ParticipantProgress::Pending(self);
                        }
                        Err(TransferFailure::Unavailable) => {
                            compatible = false;
                            break;
                        }
                    }
                }
                let Some(header) = HeadTraversal::header(zalsa, head.database_key_index) else {
                    compatible = false;
                    break;
                };
                if !self
                    .memo
                    .header
                    .provisional_head_matches(zalsa, header, head.iteration.load())
                {
                    compatible = false;
                    break;
                }
            }
        }

        let may_continue = execution.consumer.may_continue();
        if compatible {
            let preferred = if may_continue {
                local
                    .try_with_query_stack(|stack| {
                        stack
                            .iter()
                            .find(|query| {
                                query.database_key_index != key
                                    && self
                                        .traversal
                                        .heads
                                        .iter()
                                        .any(|&(key, _)| key == query.database_key_index)
                            })
                            .map(|query| query.database_key_index)
                    })
                    .flatten()
            } else {
                None
            };
            for target in preferred.into_iter().chain(
                self.traversal
                    .heads
                    .iter()
                    .map(|&(head, _)| head)
                    .filter(|target| Some(*target) != preferred),
            ) {
                if target == key {
                    continue;
                }
                match execution
                    .claim_guard
                    .retire_participant(target, !may_continue, &quote)
                {
                    Ok(refetch) => {
                        drop(self.cancellation_guard.take());
                        #[cfg(all(test, not(feature = "shuttle")))]
                        transfer_trace::record(
                            TransferEvent::new(Kind::Refetch)
                                .key(key)
                                .serial(serial)
                                .decision(refetch),
                        );
                        return ParticipantProgress::Complete(if refetch {
                            None
                        } else {
                            Some(self.memo)
                        });
                    }
                    Err((claim, TransferFailure::Requote)) => {
                        execution.claim_guard = claim;
                        self.execution = Some(execution);
                        return ParticipantProgress::Pending(self);
                    }
                    Err((claim, TransferFailure::Unavailable)) => execution.claim_guard = claim,
                }
            }
            if may_continue && execution.claim_guard.is_reclaimed_transfer() {
                let refetch = execution.claim_guard.drop();
                drop(self.cancellation_guard.take());
                #[cfg(all(test, not(feature = "shuttle")))]
                transfer_trace::record(
                    TransferEvent::new(Kind::Refetch)
                        .key(key)
                        .serial(serial)
                        .decision(refetch),
                );
                return ParticipantProgress::Complete(if refetch { None } else { Some(self.memo) });
            }
        }
        if self.cached || !compatible {
            execution.previous = Some(PreviousMemo::Semantic(self.memo));
            drop(self.cancellation_guard.take());
            return ParticipantProgress::Execute(execution);
        }
        self.execution = Some(execution);
        self.poison();
        drop(self.cancellation_guard.take());
        panic!(
            "dependency graph cycle has no enclosing read recipient or transferable convergence owner"
        );
    }

    fn poison(&mut self) {
        if self.poisoned {
            return;
        }
        if let Some(execution) = &self.execution {
            let zalsa = execution.claim_guard.zalsa();
            let id = execution.claim_guard.database_key_index().key_index();
            if execution
                .ingredient
                .get_memo_from_table_for(zalsa, id, execution.memo_ingredient_index)
                .is_some_and(|memo| memo.header.may_be_provisional())
            {
                poison_provisional_memo(
                    execution.ingredient,
                    zalsa,
                    id,
                    execution.memo_ingredient_index,
                );
            }
            self.poisoned = true;
        }
    }

    pub(in crate::function) fn abort(mut self) {
        if let Some(execution) = self.execution.take() {
            execution.claim_guard.abort();
        }
        drop(self.cancellation_guard.take());
    }

    pub(in crate::function) fn execute(mut self) -> ParticipantProgress<'db, C> {
        loop {
            let Some(work) = self.work() else {
                panic!("participant retirement work size overflow");
            };
            match self.advance(work) {
                ParticipantProgress::Pending(next) => self = next,
                result => return result,
            }
        }
    }
}

impl<C: Configuration> Drop for Participant<'_, C> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.poison();
        }
    }
}

/// An explicit transitive traversal shared by newly computed and cached participants.
/// Each selected header is checked again after admission; no user callback runs during a step.
pub(super) struct HeadTraversal<'db> {
    root: CycleHeads,
    me: DatabaseKeyIndex,
    initial: IterationStamp,
    heads: Vec<(DatabaseKeyIndex, IterationStamp)>,
    root_prefix_len: usize,
    observations: Vec<HeadObservation<'db>>,
    observed_entries: usize,
    next: usize,
    started: bool,
    max_iteration: IterationStamp,
    depends_on_self: bool,
}

pub(in crate::function) struct HeadWork<'db> {
    selected: Option<(DatabaseKeyIndex, Option<&'db MemoHeader>)>,
    entries: usize,
    pub(super) units: usize,
    pub(super) bytes: usize,
    heads_capacity: usize,
    observations_capacity: usize,
}

struct HeadObservation<'db> {
    key: DatabaseKeyIndex,
    header: &'db MemoHeader,
    heads: Vec<(DatabaseKeyIndex, IterationStamp)>,
    backing_len: usize,
    revision: crate::Revision,
    iteration: IterationStamp,
    provisional: bool,
    reuse: MemoReuse,
}

fn reserve_bytes<T>(values: &Vec<T>, required: usize) -> Option<usize> {
    if required > values.capacity() {
        required.checked_mul(size_of::<T>())
    } else {
        Some(0)
    }
}

fn reserve<T>(values: &mut Vec<T>, required: usize) {
    if required > values.capacity() {
        #[cfg(all(test, not(feature = "shuttle")))]
        tests::record_reservation(required * size_of::<T>(), values.capacity());
        values.reserve_exact(required - values.len());
    }
}

fn relocation<T>(values: &Vec<T>, required: usize) -> usize {
    if required > values.capacity() {
        values.capacity()
    } else {
        0
    }
}

fn triangular(count: usize) -> Option<usize> {
    if count == 0 {
        Some(0)
    } else {
        count.checked_mul(count - 1).map(|value| value / 2)
    }
}

impl<'db> HeadTraversal<'db> {
    pub(super) fn new(root: CycleHeads, me: DatabaseKeyIndex, iteration: IterationStamp) -> Self {
        Self {
            root,
            me,
            initial: iteration,
            heads: Vec::new(),
            root_prefix_len: 0,
            observations: Vec::new(),
            observed_entries: 0,
            next: 0,
            started: false,
            max_iteration: iteration,
            depends_on_self: false,
        }
    }

    pub(super) fn check_work(&self) -> Option<usize> {
        self.observed_entries
            .checked_add(self.observations.len())?
            .checked_add(1)
    }

    pub(super) fn work(&self, zalsa: &'db Zalsa) -> Option<HeadWork<'db>> {
        let selected = if !self.started {
            None
        } else {
            let &(key, _) = self.heads.get(self.next)?;
            Some((
                key,
                if key == self.me {
                    None
                } else {
                    Self::header(zalsa, key)
                },
            ))
        };
        let entries = match selected {
            None => self.root.storage_len(),
            Some((_, header)) => {
                header.map_or(0, |header| header.revisions.cycle_heads().storage_len())
            }
        };
        let heads_capacity = self.heads.len().checked_add(entries)?;
        let observations_capacity = self
            .observations
            .len()
            .checked_add(usize::from(matches!(selected, Some((_, Some(_))))))?;
        let (units, bytes) = if selected.is_none() {
            let units = entries
                .checked_mul(3)?
                .checked_add(triangular(entries)?)?
                .checked_add(relocation(&self.heads, heads_capacity))?
                .checked_add(self.check_work()?)?
                .checked_add(8)?;
            (units, reserve_bytes(&self.heads, heads_capacity)?)
        } else {
            let units = entries
                .checked_mul(4)?
                .checked_add(entries.checked_mul(self.heads.len())?)?
                .checked_add(triangular(entries)?)?
                .checked_add(relocation(&self.heads, heads_capacity))?
                .checked_add(relocation(&self.observations, observations_capacity))?
                .checked_add(self.observations.len())?
                .checked_add(self.check_work()?)?
                .checked_add(8)?;
            let bytes = reserve_bytes(&self.heads, heads_capacity)?
                .checked_add(reserve_bytes(&self.observations, observations_capacity)?)?
                .checked_add(
                    entries.checked_mul(size_of::<(DatabaseKeyIndex, IterationStamp)>())?,
                )?;
            (units, bytes)
        };
        Some(HeadWork {
            selected,
            entries,
            units,
            bytes,
            heads_capacity,
            observations_capacity,
        })
    }

    fn header(zalsa: &'db Zalsa, key: DatabaseKeyIndex) -> Option<&'db MemoHeader> {
        Self::memo(zalsa, key).map(|memo| memo.header())
    }

    fn memo(zalsa: &'db Zalsa, key: DatabaseKeyIndex) -> Option<ErasedMemo<'db>> {
        zalsa
            .lookup_ingredient(key.ingredient_index())
            .as_function()?
            .memo(zalsa, key.key_index())
    }

    pub(super) fn is_current(&self, zalsa: &'db Zalsa) -> bool {
        self.observations.iter().all(|observation| {
            Self::header(zalsa, observation.key).is_some_and(|current| {
                std::ptr::eq(current, observation.header)
                    && current.verified_at.load() == observation.revision
                    && current.revisions.iteration() == observation.iteration
                    && current.may_be_provisional() == observation.provisional
                    && current.attempt_reuse(zalsa) == observation.reuse
                    && current.revisions.cycle_heads().storage_len() <= observation.backing_len
                    && same_heads(current.revisions.cycle_heads(), &observation.heads)
            })
        })
    }

    pub(super) fn restart(&mut self) {
        self.heads.clear();
        self.root_prefix_len = 0;
        self.observations.clear();
        self.observed_entries = 0;
        self.next = 0;
        self.started = false;
        self.max_iteration = self.initial;
        self.depends_on_self = false;
    }

    pub(super) fn advance(&mut self, zalsa: &'db Zalsa, work: HeadWork<'db>) {
        if !self.is_current(zalsa) {
            self.restart();
            return;
        }
        let heads = match work.selected {
            None => {
                if self.root.storage_len() > work.entries {
                    return;
                }
                reserve(&mut self.heads, work.heads_capacity);
                for head in &self.root {
                    let key = head.database_key_index;
                    let iteration = head.iteration.load();
                    self.max_iteration = self.max_iteration.max(iteration);
                    self.depends_on_self |= key == self.me;
                    if !self.heads.iter().any(|&(existing, _)| existing == key) {
                        self.heads.push((key, iteration));
                    }
                }
                self.root_prefix_len = self.heads.len();
                self.started = true;
                self.normalize_self();
                return;
            }
            Some((key, selected)) => {
                let current_memo = Self::memo(zalsa, key);
                let current = current_memo.map(|memo| memo.header());
                if current.map(std::ptr::from_ref) != selected.map(std::ptr::from_ref) {
                    return;
                }
                let Some(memo) = current_memo else {
                    panic!("cycle head memo must have been created during execution");
                };
                let header = memo.header();
                let status = header.provisional_status(memo.has_value());
                if matches!(status, ProvisionalStatus::Poisoned { .. }) {
                    crate::Cancelled::PropagatedPanic.throw();
                }

                header.revisions.cycle_heads()
            }
        };
        if heads.storage_len() > work.entries {
            return;
        }
        reserve(&mut self.observations, work.observations_capacity);
        #[cfg(all(test, not(feature = "shuttle")))]
        tests::record_reservation(
            work.entries * size_of::<(DatabaseKeyIndex, IterationStamp)>(),
            0,
        );
        let mut snapshot = Vec::with_capacity(work.entries);
        snapshot.extend(
            heads
                .into_iter()
                .map(|head| (head.database_key_index, head.iteration.load())),
        );

        for &(key, iteration) in &snapshot {
            self.max_iteration = self.max_iteration.max(iteration);
            self.depends_on_self |= key == self.me;
            if !self.heads.iter().any(|&(existing, _)| existing == key) {
                reserve(&mut self.heads, work.heads_capacity);
                self.heads.push((key, iteration));
            }
        }
        if let Some((key, Some(header))) = work.selected {
            self.observed_entries += heads.storage_len();
            self.observations.push(HeadObservation {
                key,
                header,
                heads: snapshot,
                backing_len: heads.storage_len(),
                revision: header.verified_at.load(),
                iteration: header.revisions.iteration(),
                provisional: header.may_be_provisional(),
                reuse: header.attempt_reuse(zalsa),
            });
        }
        self.next += 1;
        self.normalize_self();
    }

    fn normalize_self(&mut self) {
        // Keys are unique, so at most one queued entry can be this query itself.
        // Its stamp and self-dependency were recorded when the entry was inserted.
        if self
            .heads
            .get(self.next)
            .is_some_and(|&(key, _)| key == self.me)
        {
            self.next += 1;
        }
    }

    pub(super) fn complete(&self) -> bool {
        self.started && self.next == self.heads.len()
    }

    pub(super) fn finish_bytes(&self) -> Option<usize> {
        let suffix = self.heads.len().checked_sub(self.root_prefix_len)?;
        if suffix == 0 {
            return Some(0);
        }
        self.root
            .storage_len()
            .checked_add(suffix)?
            .checked_mul(size_of::<crate::cycle::CycleHead>())?
            .checked_add(2 * size_of::<usize>())
    }

    pub(super) fn finish_work(&self) -> Option<usize> {
        let suffix = self.heads.len().checked_sub(self.root_prefix_len)?;
        let root = self.root.storage_len();
        let growth = if suffix == 0 {
            0
        } else {
            root.checked_add(suffix)?
        };
        root.checked_mul(suffix)?
            .checked_mul(2)?
            .checked_add(triangular(suffix)?.checked_mul(2)?)?
            .checked_add(suffix.checked_mul(2)?)?
            .checked_add(growth)?
            .checked_add(self.observations.len())?
            .checked_add(self.check_work()?)?
            .checked_add(8)
    }

    pub(super) fn finish(mut self) -> (CycleHeads, IterationStamp, bool) {
        let suffix = &self.heads[self.root_prefix_len..];
        if !suffix.is_empty() {
            #[cfg(all(test, not(feature = "shuttle")))]
            tests::record_root_reservation(self.root.storage_len(), suffix.len());
            self.root.reserve_additional(suffix.len());
        }
        for &(key, iteration) in suffix {
            if !self.root.contains(&key) {
                self.root.insert(key, iteration);
            }
        }
        (self.root, self.max_iteration, self.depends_on_self)
    }
}

fn same_heads(heads: &CycleHeads, snapshot: &[(DatabaseKeyIndex, IterationStamp)]) -> bool {
    heads
        .into_iter()
        .map(|head| {
            #[cfg(all(test, not(feature = "shuttle")))]
            tests::record_evidence_scan();
            (head.database_key_index, head.iteration.load())
        })
        .eq(snapshot.iter().copied())
}
