use rustc_hash::FxHashMap;
use std::collections::hash_map::OccupiedEntry;

#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Action, Event, Kind, Mode, SyncSnapshot,
};

use crate::key::DatabaseKeyIndex;
use crate::plumbing::ZalsaLocal;
use crate::runtime::{
    BlockOnTransferredOwner, BlockResult, BlockTransferredResult, RetirementQuote, Running,
    TransferFailure, WaitResult,
};
use crate::sync::Mutex;
use crate::sync::thread::{self};
use crate::tracing;
use crate::zalsa::Zalsa;
use crate::{Id, IngredientIndex};

pub(crate) type SyncGuard<'me> = crate::sync::MutexGuard<'me, FxHashMap<Id, SyncState>>;

/// Keeps a failed checked transfer from retiring the live source claim.
pub(crate) struct TransferSource<'a> {
    guard: SyncGuard<'a>,
    claim: &'a ClaimGuard<'a>,
}

impl<'a> TransferSource<'a> {
    pub(crate) fn commit(mut self) -> SyncGuard<'a> {
        let state = self
            .guard
            .get_mut(&self.claim.key_index)
            .expect("the retained source claim owns its sync entry");
        #[cfg(all(test, not(feature = "shuttle")))]
        self.claim
            .transfer_test_record(Kind::Terminal, Some(state), Some(Action::Drop), None);
        state.id = SyncOwner::Transferred;
        state.claimed_twice = false;
        #[cfg(all(test, not(feature = "shuttle")))]
        self.claim
            .transfer_test_record(Kind::TransferBegin, Some(state), None, None);
        self.guard
    }
}

/// Tracks the keys that are currently being processed; used to coordinate between
/// worker threads.
pub(crate) struct SyncTable {
    syncs: Mutex<FxHashMap<Id, SyncState>>,
    ingredient: IngredientIndex,
}

pub(crate) enum ClaimResult<'a, Guard = ClaimGuard<'a>> {
    /// Successfully claimed the query.
    Claimed(Guard),
    /// Can't claim the query because it is running on an other thread.
    Running(Running<'a>),
    /// Claiming the query results in a cycle.
    Cycle {
        /// `true` if this is a cycle with an inner query. For example, if `a` transferred its ownership to
        /// `b`. If the thread claiming `b` tries to claim `a`, then this results in a cycle except when calling
        /// [`SyncTable::try_claim`] with [`Reentrant::Allow`].
        inner: bool,
    },
}

pub(crate) struct SyncState {
    /// The thread id that currently owns this query (actively executing it or iterating it as part of a larger cycle).
    id: SyncOwner,

    /// Set to true if any other queries are blocked,
    /// waiting for this query to complete.
    anyone_waiting: bool,

    /// Whether any other query has transferred its lock ownership to this query.
    /// This is only an optimization so that the expensive unblocking of transferred queries
    /// can be skipped if `false`. This field might be `true` in cases where queries *were* transferred
    /// to this query, but have since then been transferred to another query (in a later iteration).
    is_transfer_target: bool,

    /// Whether this query has been claimed by the query that currently owns it.
    ///
    /// If `a` has been transferred to `b` and the stack for t1 is `b -> a`, then `a` can be claimed
    /// and `claimed_twice` is set to `true`. However, t2 won't be able to claim `a` because
    /// it doesn't own `b`.
    claimed_twice: bool,
}

#[cfg(all(test, not(feature = "shuttle")))]
impl SyncState {
    fn transfer_test_snapshot(&self) -> SyncSnapshot {
        SyncSnapshot {
            owner: self.id,
            anyone_waiting: self.anyone_waiting,
            is_transfer_target: self.is_transfer_target,
            claimed_twice: self.claimed_twice,
        }
    }
}

impl SyncTable {
    #[cfg(all(test, not(feature = "shuttle")))]
    pub(crate) fn test_transfer_state(&self, id: Id) -> Option<SyncSnapshot> {
        self.syncs
            .lock()
            .get(&id)
            .map(SyncState::transfer_test_snapshot)
    }

    pub(crate) fn new(ingredient: IngredientIndex) -> Self {
        Self {
            syncs: Default::default(),
            ingredient,
        }
    }

    /// Claims the given key index, or blocks if it is running on another thread.
    pub(crate) fn try_claim<'me>(
        &'me self,
        zalsa: &'me Zalsa,
        zalsa_local: &'me ZalsaLocal,
        key_index: Id,
        reentrant: Reentrancy,
    ) -> ClaimResult<'me> {
        let mut write = self.syncs.lock();
        match write.entry(key_index) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                let id = match occupied_entry.get().id {
                    SyncOwner::Thread(id) => id,
                    SyncOwner::Transferred => {
                        return match self.try_claim_transferred(
                            zalsa,
                            zalsa_local,
                            occupied_entry,
                            reentrant,
                        ) {
                            Ok(claimed) => claimed,
                            Err(other_thread) => match other_thread.block(write) {
                                BlockResult::Cycle => ClaimResult::Cycle { inner: false },
                                BlockResult::Running(running) => ClaimResult::Running(running),
                            },
                        };
                    }
                };

                let SyncState { anyone_waiting, .. } = occupied_entry.into_mut();

                // NB: `Ordering::Relaxed` is sufficient here,
                // as there are no loads that are "gated" on this
                // value. Everything that is written is also protected
                // by a lock that must be acquired. The role of this
                // boolean is to decide *whether* to acquire the lock,
                // not to gate future atomic reads.
                *anyone_waiting = true;
                match zalsa.runtime().block(
                    DatabaseKeyIndex::new(self.ingredient, key_index),
                    id,
                    write,
                ) {
                    BlockResult::Running(blocked_on) => ClaimResult::Running(blocked_on),
                    BlockResult::Cycle => ClaimResult::Cycle { inner: false },
                }
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let state = vacant_entry.insert(SyncState {
                    id: SyncOwner::Thread(thread::current().id()),
                    anyone_waiting: false,
                    is_transfer_target: false,
                    claimed_twice: false,
                });
                let claim = ClaimGuard {
                    #[cfg(test)]
                    test_serial: ClaimGuard::next_test_serial(),
                    key_index,
                    zalsa,
                    zalsa_local,
                    sync_table: self,
                    mode: ReleaseMode::Default,
                };
                #[cfg(all(test, not(feature = "shuttle")))]
                claim.transfer_test_record(Kind::Claim, Some(state), None, None);
                #[cfg(not(all(test, not(feature = "shuttle"))))]
                let _ = state;
                ClaimResult::Claimed(claim)
            }
        }
    }

    /// Claims the given key index, or blocks if it is running on another thread.
    pub(crate) fn peek_claim<'me>(
        &'me self,
        zalsa: &'me Zalsa,
        key_index: Id,
        reentrant: Reentrancy,
    ) -> ClaimResult<'me, ()> {
        let mut write = self.syncs.lock();
        match write.entry(key_index) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                let id = match occupied_entry.get().id {
                    SyncOwner::Thread(id) => id,
                    SyncOwner::Transferred => {
                        return match self.peek_claim_transferred(zalsa, occupied_entry, reentrant) {
                            Ok(claimed) => claimed,
                            Err(other_thread) => match other_thread.block(write) {
                                BlockResult::Cycle => ClaimResult::Cycle { inner: false },
                                BlockResult::Running(running) => ClaimResult::Running(running),
                            },
                        };
                    }
                };

                let SyncState { anyone_waiting, .. } = occupied_entry.into_mut();

                // NB: `Ordering::Relaxed` is sufficient here,
                // as there are no loads that are "gated" on this
                // value. Everything that is written is also protected
                // by a lock that must be acquired. The role of this
                // boolean is to decide *whether* to acquire the lock,
                // not to gate future atomic reads.
                *anyone_waiting = true;
                match zalsa.runtime().block(
                    DatabaseKeyIndex::new(self.ingredient, key_index),
                    id,
                    write,
                ) {
                    BlockResult::Running(blocked_on) => ClaimResult::Running(blocked_on),
                    BlockResult::Cycle => ClaimResult::Cycle { inner: false },
                }
            }
            std::collections::hash_map::Entry::Vacant(_) => ClaimResult::Claimed(()),
        }
    }

    #[cold]
    #[inline(never)]
    fn try_claim_transferred<'me>(
        &'me self,
        zalsa: &'me Zalsa,
        zalsa_local: &'me ZalsaLocal,
        mut entry: OccupiedEntry<Id, SyncState>,
        reentrant: Reentrancy,
    ) -> Result<ClaimResult<'me>, Box<BlockOnTransferredOwner<'me>>> {
        let key_index = *entry.key();
        let database_key_index = DatabaseKeyIndex::new(self.ingredient, key_index);
        let thread_id = thread::current().id();

        match zalsa
            .runtime()
            .block_transferred(database_key_index, thread_id)
        {
            BlockTransferredResult::ImTheOwner if reentrant.is_allow() => {
                let state = entry.into_mut();
                let SyncState {
                    id, claimed_twice, ..
                } = state;
                debug_assert!(!*claimed_twice);

                *id = SyncOwner::Thread(thread_id);
                *claimed_twice = true;

                let claim = ClaimGuard {
                    #[cfg(test)]
                    test_serial: ClaimGuard::next_test_serial(),
                    key_index,
                    zalsa,
                    zalsa_local,
                    sync_table: self,
                    mode: ReleaseMode::SelfOnly,
                };
                #[cfg(all(test, not(feature = "shuttle")))]
                claim.transfer_test_record(Kind::Claim, Some(state), None, None);
                Ok(ClaimResult::Claimed(claim))
            }
            BlockTransferredResult::ImTheOwner => Ok(ClaimResult::Cycle { inner: true }),
            BlockTransferredResult::OwnedBy(other_thread) => {
                entry.get_mut().anyone_waiting = true;
                Err(other_thread)
            }
            BlockTransferredResult::Released => {
                entry.insert(SyncState {
                    id: SyncOwner::Thread(thread_id),
                    anyone_waiting: false,
                    is_transfer_target: false,
                    claimed_twice: false,
                });
                let claim = ClaimGuard {
                    #[cfg(test)]
                    test_serial: ClaimGuard::next_test_serial(),
                    key_index,
                    zalsa,
                    zalsa_local,
                    sync_table: self,
                    mode: ReleaseMode::Default,
                };
                #[cfg(all(test, not(feature = "shuttle")))]
                claim.transfer_test_record(Kind::Claim, Some(entry.get()), None, None);
                Ok(ClaimResult::Claimed(claim))
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn peek_claim_transferred<'me>(
        &'me self,
        zalsa: &'me Zalsa,
        mut entry: OccupiedEntry<Id, SyncState>,
        reentrant: Reentrancy,
    ) -> Result<ClaimResult<'me, ()>, Box<BlockOnTransferredOwner<'me>>> {
        let key_index = *entry.key();
        let database_key_index = DatabaseKeyIndex::new(self.ingredient, key_index);
        let thread_id = thread::current().id();

        match zalsa
            .runtime()
            .block_transferred(database_key_index, thread_id)
        {
            BlockTransferredResult::ImTheOwner if reentrant.is_allow() => {
                Ok(ClaimResult::Claimed(()))
            }
            BlockTransferredResult::ImTheOwner => Ok(ClaimResult::Cycle { inner: true }),
            BlockTransferredResult::OwnedBy(other_thread) => {
                entry.get_mut().anyone_waiting = true;
                Err(other_thread)
            }
            BlockTransferredResult::Released => Ok(ClaimResult::Claimed(())),
        }
    }

    /// Marks `key_index` as a transfer target.
    ///
    /// Returns the `SyncOwnerId` of the thread that currently owns this query.
    ///
    /// Note: The result of this method will immediately become stale unless the thread owning `key_index`
    /// is currently blocked on this thread (claiming `key_index` from this thread results in a cycle).
    fn checked_transfer_target(
        &self,
        zalsa: &Zalsa,
        query: DatabaseKeyIndex,
        key_index: Id,
        quote: &RetirementQuote,
    ) -> Result<SyncOwner, TransferFailure> {
        let mut syncs = self.syncs.lock();
        let Some(state) = syncs.get_mut(&key_index) else {
            return Err(TransferFailure::Unavailable);
        };
        zalsa.runtime().check_transfer_target(
            query,
            DatabaseKeyIndex::new(self.ingredient, key_index),
            state.id,
            quote,
        )?;
        state.anyone_waiting = true;
        state.is_transfer_target = true;
        Ok(state.id)
    }

    pub(super) fn mark_as_transfer_target(&self, key_index: Id) -> Option<SyncOwner> {
        let mut syncs = self.syncs.lock();
        syncs.get_mut(&key_index).map(|state| {
            // We set `anyone_waiting` to true because it is used in `ClaimGuard::release`
            // to exit early if the query doesn't need to release any locks.
            // However, there are now dependent queries that need to be released, that's why we set `anyone_waiting` to true,
            // so that `ClaimGuard::release` no longer exits early.
            state.anyone_waiting = true;
            state.is_transfer_target = true;

            state.id
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum SyncOwner {
    /// Query is owned by this thread
    Thread(thread::ThreadId),

    /// The query's lock ownership has been transferred to another query.
    /// E.g. if `a` transfers its ownership to `b`, then only the thread in the critical path
    /// to complete `b` can claim `a` (in most instances, only the thread owning `b` can claim `a`).
    ///
    /// The thread owning `a` is stored in the `DependencyGraph`.
    ///
    /// A query can be marked as `Transferred` even if it has since then been released by the owning query.
    /// In that case, the query is effectively unclaimed and the `Transferred` state is stale. The reason
    /// for this is that it avoids the need for locking each sync table when releasing the transferred queries.
    Transferred,
}

/// Marks an active 'claim' in the synchronization map. The claim is
/// released when this value is dropped.
#[must_use]
pub(crate) struct ClaimGuard<'me> {
    #[cfg(test)]
    test_serial: usize,
    key_index: Id,
    zalsa: &'me Zalsa,
    sync_table: &'me SyncTable,
    mode: ReleaseMode,
    zalsa_local: &'me ZalsaLocal,
}

impl<'me> ClaimGuard<'me> {
    pub(crate) fn check_participant_head(
        &self,
        target: DatabaseKeyIndex,
        quote: &RetirementQuote,
    ) -> Result<(), TransferFailure> {
        let Some(function) = self
            .zalsa
            .lookup_ingredient(target.ingredient_index())
            .as_function()
        else {
            return Err(TransferFailure::Unavailable);
        };
        function
            .sync_table()
            .checked_transfer_target(
                self.zalsa,
                self.database_key_index(),
                target.key_index(),
                quote,
            )
            .map(|_| ())
    }

    pub(crate) fn is_reclaimed_transfer(&self) -> bool {
        matches!(self.mode, ReleaseMode::SelfOnly)
    }

    pub(crate) fn retire_participant(
        mut self,
        target: DatabaseKeyIndex,
        must_refetch: bool,
        quote: &RetirementQuote,
    ) -> Result<bool, (Self, TransferFailure)> {
        let Some(function) = self
            .zalsa
            .lookup_ingredient(target.ingredient_index())
            .as_function()
        else {
            return Err((self, TransferFailure::Unavailable));
        };
        let owner = match function.sync_table().checked_transfer_target(
            self.zalsa,
            self.database_key_index(),
            target.key_index(),
            quote,
        ) {
            Ok(owner) => owner,
            Err(error) => return Err((self, error)),
        };
        let original_mode = self.mode;
        self.mode = ReleaseMode::TransferTo(target);
        let source = TransferSource {
            guard: self.sync_table.syncs.lock(),
            claim: &self,
        };

        let result = self.zalsa.runtime().transfer_lock_checked(
            self.database_key_index(),
            target,
            owner,
            source,
            must_refetch,
            quote,
        );
        match result {
            Ok(refetch) => {
                #[cfg(all(test, not(feature = "shuttle")))]
                {
                    transfer_trace::record(
                        Event::new(Kind::TransferEnd)
                            .key(self.database_key_index())
                            .serial(self.test_serial)
                            .decision(refetch),
                    );
                }
                std::mem::forget(self);
                Ok(refetch)
            }
            Err(error) => {
                self.mode = original_mode;
                Err((self, error))
            }
        }
    }

    #[cfg(all(test, not(feature = "shuttle")))]
    fn transfer_test_record(
        &self,
        kind: Kind,
        state: Option<&SyncState>,
        action: Option<Action>,
        wait: Option<WaitResult>,
    ) {
        let mut event = Event::new(kind)
            .key(self.database_key_index())
            .serial(self.test_serial);
        event.mode = Some(match self.mode {
            ReleaseMode::Default => Mode::Default,
            ReleaseMode::SelfOnly => Mode::SelfOnly,
            ReleaseMode::TransferTo(key) => Mode::TransferTo(key),
        });
        event.sync = state.map(SyncState::transfer_test_snapshot);
        event.action = action;
        event.wait = wait;
        transfer_trace::record(event);
    }

    #[cfg(test)]
    fn next_test_serial() -> usize {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(in crate::function) fn test_serial(&self) -> usize {
        self.test_serial
    }

    pub(crate) const fn zalsa(&self) -> &'me Zalsa {
        self.zalsa
    }

    pub(crate) fn zalsa_local(&self) -> &'me ZalsaLocal {
        self.zalsa_local
    }

    pub(crate) const fn database_key_index(&self) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.sync_table.ingredient, self.key_index)
    }

    pub(crate) fn set_release_mode(&mut self, mode: ReleaseMode) {
        self.mode = mode;
        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(Kind::Mode, None, None, None);
    }

    #[cold]
    #[inline(never)]
    fn release_panicking(&self) {
        let mut syncs = self.sync_table.syncs.lock();
        let state = syncs.remove(&self.key_index).expect("key claimed twice?");
        let result = if self.zalsa_local.should_trigger_local_cancellation() {
            WaitResult::Cancelled
        } else {
            WaitResult::Panicked
        };
        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(
            Kind::Terminal,
            Some(&state),
            Some(Action::Panic),
            Some(result),
        );
        tracing::debug!(
            "Release claim on {:?} due to {:?}",
            self.database_key_index(),
            result
        );
        self.release(state, result);
    }

    #[inline(always)]
    fn release(&self, state: SyncState, wait_result: WaitResult) {
        let SyncState {
            anyone_waiting,
            is_transfer_target,
            claimed_twice,
            ..
        } = state;

        if !anyone_waiting {
            return;
        }

        let runtime = self.zalsa.runtime();
        let database_key_index = self.database_key_index();

        if claimed_twice {
            runtime.undo_transfer_lock(database_key_index);
        }

        runtime.unblock_queries_blocked_on(database_key_index, wait_result);

        if is_transfer_target {
            runtime.unblock_transferred_queries_owned_by(database_key_index, wait_result);
        }
    }

    #[cold]
    #[inline(never)]
    fn release_self(&self) {
        let mut syncs = self.sync_table.syncs.lock();
        let std::collections::hash_map::Entry::Occupied(mut state) = syncs.entry(self.key_index)
        else {
            panic!("key should only be claimed/released once");
        };

        if state.get().claimed_twice {
            #[cfg(all(test, not(feature = "shuttle")))]
            self.transfer_test_record(
                Kind::Terminal,
                Some(state.get()),
                Some(Action::Drop),
                Some(WaitResult::Completed),
            );
            state.get_mut().claimed_twice = false;
            state.get_mut().id = SyncOwner::Transferred;
            #[cfg(all(test, not(feature = "shuttle")))]
            self.transfer_test_record(Kind::Restore, Some(state.get()), None, None);
        } else {
            #[cfg(all(test, not(feature = "shuttle")))]
            self.transfer_test_record(
                Kind::Terminal,
                Some(state.get()),
                Some(Action::Drop),
                Some(WaitResult::Completed),
            );
            self.release(state.remove(), WaitResult::Completed);
        }
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn transfer(&self, new_owner: DatabaseKeyIndex) -> bool {
        // Get the owning thread of `new_owner`.
        // The thread id is guaranteed to not be stale because `new_owner` must be blocked on `self_key`
        // or `transfer_lock` will panic (at least in debug builds).
        let Some(new_owner_thread_id) = self
            .zalsa
            .lookup_ingredient(new_owner.ingredient_index())
            .as_function()
            .expect("lock owners must be function ingredients")
            .sync_table()
            .mark_as_transfer_target(new_owner.key_index())
        else {
            self.release(
                self.sync_table
                    .syncs
                    .lock()
                    .remove(&self.key_index)
                    .expect("key should only be claimed/released once"),
                WaitResult::Panicked,
            );

            panic!("new owner to be a locked query")
        };

        let mut syncs = self.sync_table.syncs.lock();

        let self_key = self.database_key_index();
        tracing::debug!(
            "Transferring lock ownership of {self_key:?} to {new_owner:?} ({new_owner_thread_id:?})"
        );

        let state = syncs
            .get_mut(&self.key_index)
            .expect("key should only be claimed/released once");
        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(Kind::Terminal, Some(state), Some(Action::Drop), None);
        let SyncState {
            id, claimed_twice, ..
        } = state;

        *id = SyncOwner::Transferred;
        *claimed_twice = false;

        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(Kind::TransferBegin, Some(state), None, None);
        let refetch =
            self.zalsa
                .runtime()
                .transfer_lock(self_key, new_owner, new_owner_thread_id, syncs);
        #[cfg(all(test, not(feature = "shuttle")))]
        transfer_trace::record(
            Event::new(Kind::TransferEnd)
                .key(self_key)
                .serial(self.test_serial)
                .decision(refetch),
        );
        refetch
    }

    /// Drops the claim on the memo.
    ///
    /// Returns `true` if the lock was transferred to another query and
    /// this thread blocked waiting for the new owner's lock to be released.
    /// In that case, any computed memo need to be refetched because they may have
    /// changed since `drop` was called.
    pub(crate) fn drop(mut self) -> bool {
        let refetch = self.drop_impl();
        std::mem::forget(self);
        refetch
    }

    /// Releases an interrupted return-only execution without waiting for a transfer.
    /// Its attempt must already be incomplete, so provisional readers cannot use it as success.
    pub(crate) fn abort(self) {
        let mut syncs = self.sync_table.syncs.lock();
        let state = syncs
            .remove(&self.key_index)
            .expect("key should only be claimed/released once");
        #[cfg(all(test, not(feature = "shuttle")))]
        self.transfer_test_record(
            Kind::Terminal,
            Some(&state),
            Some(Action::Abort),
            Some(WaitResult::Cancelled),
        );
        self.release(state, WaitResult::Cancelled);
        drop(syncs);
        std::mem::forget(self);
    }

    fn drop_impl(&mut self) -> bool {
        match self.mode {
            ReleaseMode::Default => {
                let mut syncs = self.sync_table.syncs.lock();
                let state = syncs
                    .remove(&self.key_index)
                    .expect("key should only be claimed/released once");

                #[cfg(all(test, not(feature = "shuttle")))]
                self.transfer_test_record(
                    Kind::Terminal,
                    Some(&state),
                    Some(Action::Drop),
                    Some(WaitResult::Completed),
                );
                self.release(state, WaitResult::Completed);
                false
            }
            ReleaseMode::SelfOnly => {
                self.release_self();
                false
            }
            ReleaseMode::TransferTo(new_owner) => self.transfer(new_owner),
        }
    }
}

impl Drop for ClaimGuard<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.release_panicking();
            return;
        }

        self.drop_impl();
    }
}

impl std::fmt::Debug for SyncTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncTable").finish()
    }
}

/// Controls how the lock is released when the `ClaimGuard` is dropped.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum ReleaseMode {
    /// The default release mode.
    ///
    /// Releases the query for which this claim guard holds the lock and any queries that have
    /// transferred ownership to this query.
    #[default]
    Default,

    /// Only releases the lock for this query. Any query that has transferred ownership to this query
    /// will remain locked.
    ///
    /// If this thread panics, the query will be released as normal (default mode).
    SelfOnly,

    /// Transfers the ownership of the lock to the specified query.
    ///
    /// The query will remain locked and only the thread owning the transfer target will be resumed.
    ///
    /// The transfer target must be a query that's blocked on this query to guarantee that the transfer target doesn't complete
    /// before the transfer is finished (which would leave this query locked forever).
    ///
    /// If this thread panics, the query will be released as normal (default mode).
    TransferTo(DatabaseKeyIndex),
}

impl std::fmt::Debug for ClaimGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaimGuard")
            .field("key_index", &self.key_index)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// Controls whether this thread can claim a query that transferred its ownership to a query
/// this thread currently holds the lock for.
///
/// For example: if query `a` transferred its ownership to query `b`, and this thread holds
/// the lock for `b`, then this thread can also claim `a` — but only when using [`Self::Allow`].
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Reentrancy {
    /// Allow `try_claim` to reclaim a query's that transferred its ownership to a query
    /// hold by this thread.
    Allow,

    /// Only allow claiming queries that haven't been claimed by any thread.
    Deny,
}

impl Reentrancy {
    const fn is_allow(self) -> bool {
        matches!(self, Reentrancy::Allow)
    }
}
