use std::pin::Pin;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Event, GraphSnapshot, Kind, Slots,
};

use crate::function::{SyncGuard, SyncOwner, TransferSource};
use crate::key::DatabaseKeyIndex;
use crate::runtime::dependency_graph::edge::EdgeCondvar;
use crate::runtime::{RetirementQuote, TransferFailure, WaitResult};
use crate::sync::MutexGuard;
use crate::sync::thread::ThreadId;
use crate::tracing;

type QueryDependents = FxHashMap<DatabaseKeyIndex, SmallVec<[ThreadId; 4]>>;
type TransferredDependents = FxHashMap<DatabaseKeyIndex, SmallSet<DatabaseKeyIndex, 4>>;

#[cfg(all(test, not(feature = "shuttle")))]
mod tests;

#[derive(Debug, Default)]
pub(super) struct DependencyGraph {
    /// A `(K -> V)` pair in this map indicates that the runtime
    /// `K` is blocked on some query executing in the runtime `V`.
    /// This encodes a graph that must be acyclic (or else deadlock
    /// will result).
    edges: Edges,

    /// Encodes the `ThreadId` that are blocked waiting for the result
    /// of a given query.
    query_dependents: QueryDependents,

    /// When a key K completes which had dependent queries Qs blocked on it,
    /// it stores its `WaitResult` here. As they wake up, each query Q in Qs will
    /// come here to fetch their results.
    wait_results: FxHashMap<ThreadId, WaitResult>,

    /// A `K -> Q` pair indicates that the query `K`'s lock is now owned by the query
    /// `Q`. It's important that `transferred` always forms a tree (must be acyclic),
    /// or else deadlock will result.
    transferred: FxHashMap<DatabaseKeyIndex, (ThreadId, DatabaseKeyIndex)>,

    /// A `K -> [Q]` pair indicates that the query `K` owns the locks of
    /// `Q`. This is the reverse mapping of `transferred` to allow efficient unlocking
    /// of all dependent queries when `K` completes.
    transferred_dependents: TransferredDependents,

    #[cfg(all(test, not(feature = "shuttle")))]
    release_counts: tests::ReleaseCounts,
}

impl DependencyGraph {
    pub(super) fn retirement_quote(&self, candidates: usize) -> Option<RetirementQuote> {
        let transfers = self.transferred.len();
        let edges = self.edges.0.len();
        // Each candidate walks transfer chains and a subtree, and may compare every waiter
        // with the complete dependency path. Include the successful rewrite and wait setup.
        let paths = edges.checked_add(1)?.checked_pow(2)?;
        let per_candidate = transfers
            .checked_add(1)?
            .checked_mul(16)?
            .checked_add(paths.checked_mul(8)?)?
            .checked_add(64)?;
        let units = candidates.checked_add(1)?.checked_mul(per_candidate)?;
        // Each failed candidate can traverse its source subtree. A successful transfer
        // can allocate independent cut and remapping cursors. Vec growth includes its
        // minimum four-element allocation and every relocation request along the chain.
        let scratch_bytes = if transfers == 0 && edges == 0 {
            // Only a same-thread vacant insertion can succeed, without a traversal cursor.
            0
        } else {
            transfers
                .checked_add(1)?
                .checked_mul(4)?
                .checked_add(8)?
                .checked_mul(candidates.checked_add(2)?)?
                .checked_mul(size_of::<std::slice::Iter<'_, DatabaseKeyIndex>>())?
        };
        Some(RetirementQuote {
            scratch_bytes,

            transfers,
            edges,
            units,
        })
    }

    pub(super) fn check_transfer_target(
        &self,
        query: DatabaseKeyIndex,
        target: DatabaseKeyIndex,
        owner: SyncOwner,
        current: ThreadId,
        quote: &RetirementQuote,
    ) -> Result<(), TransferFailure> {
        if self.transferred.len() > quote.transfers || self.edges.0.len() > quote.edges {
            return Err(TransferFailure::Requote);
        }
        if query == target {
            return Err(TransferFailure::Unavailable);
        }
        if !self.transferred.contains_key(&query) {
            let mut at = target;
            while let Some(&(_, next)) = self.transferred.get(&at) {
                #[cfg(all(test, not(feature = "shuttle")))]
                tests::count_work(tests::WorkKind::Chain, 1);

                if next == query {
                    return Err(TransferFailure::Unavailable);
                }
                at = next;
            }
        }

        let target_thread = match owner {
            SyncOwner::Thread(thread) => Some(thread),
            SyncOwner::Transferred => self.thread_id_of_transferred_query(target, Some(query)),
        };
        match target_thread {
            Some(thread) if thread == current || self.depends_on(thread, current) => Ok(()),
            _ => Err(TransferFailure::Unavailable),
        }
    }

    pub(super) fn transfer_lock_checked(
        me: MutexGuard<Self>,
        query: DatabaseKeyIndex,
        current_thread: ThreadId,
        target: DatabaseKeyIndex,
        owner: SyncOwner,
        source: TransferSource<'_>,
        must_refetch: bool,
        quote: &RetirementQuote,
    ) -> Result<bool, TransferFailure> {
        me.check_transfer_target(query, target, owner, current_thread, quote)?;
        let target_thread = match owner {
            SyncOwner::Thread(thread) => Some(thread),
            SyncOwner::Transferred => me.thread_id_of_transferred_query(target, Some(query)),
        };
        let Some(target_thread) = target_thread else {
            return Err(TransferFailure::Unavailable);
        };
        if target_thread != current_thread && !me.depends_on(target_thread, current_thread) {
            return Err(TransferFailure::Unavailable);
        }
        let cut = if must_refetch {
            if target_thread == current_thread {
                return Err(TransferFailure::Unavailable);
            }
            // A rotation detaches the target-containing branch from the source subtree.
            // A waiter in that branch is not one the canonical source transfer will release.
            let mut detached = None;
            if me.transferred.contains_key(&query) {
                let mut at = target;
                while let Some(&(_, next)) = me.transferred.get(&at) {
                    #[cfg(all(test, not(feature = "shuttle")))]
                    tests::count_work(tests::WorkKind::Chain, 1);

                    if next == query {
                        detached = Some(at);
                        break;
                    }
                    at = next;
                }
            }
            let Some(cut) = me.transfer_waiter(query, target_thread, detached) else {
                return Err(TransferFailure::Unavailable);
            };
            Some(cut)
        } else {
            None
        };
        let guard = source.commit();
        Ok(Self::transfer_lock_with_cut(
            me,
            query,
            current_thread,
            target,
            owner,
            guard,
            cut,
        ))
    }

    #[cfg(all(test, not(feature = "shuttle")))]
    pub(super) fn test_transfer_snapshot(&self) -> GraphSnapshot {
        let mut snapshot = GraphSnapshot::default();
        for (&from, edge) in &self.edges.0 {
            snapshot.edges.push((from, edge.blocked_on_id));
        }
        for (&key, dependents) in &self.query_dependents {
            let mut threads = Slots::default();
            for &thread in dependents {
                threads.push(thread);
            }
            snapshot.dependents.push((key, threads));
        }
        for (&thread, &result) in &self.wait_results {
            snapshot.pending.push((thread, result));
        }
        for (&key, &(thread, owner)) in &self.transferred {
            snapshot.transferred.push((key, thread, owner));
        }
        for (&key, dependents) in &self.transferred_dependents {
            let mut keys = Slots::default();
            for &dependent in dependents {
                keys.push(dependent);
            }
            snapshot.reverse.push((key, keys));
        }
        snapshot
    }

    #[cfg(all(test, not(feature = "shuttle")))]
    fn transfer_test_mapping(
        kind: Kind,
        query: DatabaseKeyIndex,
        thread: ThreadId,
        owner: DatabaseKeyIndex,
    ) {
        let mut event = Event::new(kind).key(query);
        event.other_key = Some(owner);
        event.peer = Some(thread);
        transfer_trace::record(event);
    }

    /// True if `from_id` depends on `to_id`.
    ///
    /// (i.e., there is a path from `from_id` to `to_id` in the graph.)
    pub(super) fn depends_on(&self, from_id: ThreadId, to_id: ThreadId) -> bool {
        self.edges.depends_on(from_id, to_id)
    }

    /// Modifies the graph so that `from_id` is blocked
    /// on `database_key`, which is being computed by
    /// `to_id`.
    ///
    /// For this to be reasonable, the lock on the
    /// results table for `database_key` must be held.
    /// This ensures that computing `database_key` doesn't
    /// complete before `block_on` executes.
    ///
    /// Preconditions:
    /// * No path from `to_id` to `from_id`
    ///   (i.e., `me.depends_on(to_id, from_id)` is false)
    /// * `held_mutex` is a read lock (or stronger) on `database_key`
    pub(super) fn block_on<QueryMutexGuard>(
        mut me: MutexGuard<'_, Self>,
        from_id: ThreadId,
        database_key: DatabaseKeyIndex,
        to_id: ThreadId,
        query_mutex_guard: QueryMutexGuard,
    ) -> WaitResult {
        let cvar = std::pin::pin!(EdgeCondvar::default());
        let cvar = cvar.as_ref();
        // SAFETY: We are blocking until the result is removed from `DependencyGraph::wait_results`
        // at which point the `edge` won't signal the condvar anymore.
        // As such we are keeping the cond var alive until the reference in the edge drops.
        unsafe { me.add_edge(from_id, database_key, to_id, cvar) };

        // Release the mutex that prevents `database_key`
        // from completing, now that the edge has been added.
        drop(query_mutex_guard);

        loop {
            if let Some(result) = me.wait_results.remove(&from_id) {
                #[cfg(all(test, not(feature = "shuttle")))]
                {
                    let mut event = Event::new(Kind::WaitConsumed).key(database_key);
                    event.from = Some(from_id);
                    event.peer = Some(to_id);
                    event.wait = Some(result);
                    transfer_trace::record(event);
                }
                debug_assert!(!me.edges.contains_key(&from_id));
                return result;
            }
            me = cvar.wait(me);
        }
    }

    /// Helper for `block_on`: performs actual graph modification
    /// to add a dependency edge from `from_id` to `to_id`, which is
    /// computing `database_key`.
    ///
    /// # Safety
    ///
    /// The caller needs to keep the referent of `cvar` alive until the corresponding
    /// [`Self::wait_results`] entry has been inserted.
    unsafe fn add_edge(
        &mut self,
        from_id: ThreadId,
        database_key: DatabaseKeyIndex,
        to_id: ThreadId,
        cvar: Pin<&EdgeCondvar>,
    ) {
        assert_ne!(from_id, to_id);
        debug_assert!(!self.edges.contains_key(&from_id));
        debug_assert!(!self.depends_on(to_id, from_id));
        // SAFETY: The caller is responsible for ensuring that the `EdgeGuard` outlives the `Edge`.
        let edge = unsafe { edge::Edge::new(to_id, cvar) };
        self.edges.insert(from_id, edge);
        self.query_dependents
            .entry(database_key)
            .or_default()
            .push(from_id);
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = Event::new(Kind::Edge).key(database_key);
            event.from = Some(from_id);
            event.peer = Some(to_id);
            transfer_trace::record(event);
        }
    }

    /// Invoked when runtime `to_id` completes executing
    /// `database_key`.
    pub(super) fn unblock_runtimes_blocked_on(
        &mut self,
        database_key: DatabaseKeyIndex,
        wait_result: WaitResult,
    ) {
        let dependents = self
            .query_dependents
            .remove(&database_key)
            .unwrap_or_default();

        for from_id in dependents {
            self.unblock_runtime(from_id, wait_result);
        }
    }

    /// Unblock the runtime with the given id with the given wait-result.
    /// This will cause it resume execution (though it will have to grab
    /// the lock on this data structure first, to recover the wait result).
    fn unblock_runtime(&mut self, id: ThreadId, wait_result: WaitResult) {
        let edge = self.edges.remove(&id).expect("not blocked");
        self.wait_results.insert(id, wait_result);
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            self.release_counts.waiters += 1;
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = Event::new(Kind::Unblock);
            event.from = Some(id);
            event.peer = Some(edge.blocked_on_id);
            event.wait = Some(wait_result);
            transfer_trace::record(event);
        }

        // Now that we have inserted the `wait_results`,
        // notify the thread.
        edge.notify();
    }

    /// Invoked when the query `database_key` completes and it owns the locks of other queries
    /// (the queries transferred their locks to `database_key`).
    pub(super) fn unblock_runtimes_blocked_on_transferred_queries_owned_by(
        &mut self,
        database_key: DatabaseKeyIndex,
        wait_result: WaitResult,
    ) {
        // If `database_key` is `c` and it has been transferred to `b` earlier, remove its entry.
        tracing::trace!(
            "unblock_runtimes_blocked_on_transferred_queries_owned_by({database_key:?}"
        );

        if let Some((_thread, owner)) = self.transferred.remove(&database_key) {
            #[cfg(all(test, not(feature = "shuttle")))]
            Self::transfer_test_mapping(Kind::MappingRemoved, database_key, _thread, owner);
            // If this query previously transferred its lock ownership to another query, remove
            // it from that queries dependents as it is now completing.
            self.transferred_dependents
                .get_mut(&owner)
                .unwrap()
                .remove(&database_key);
        }

        // Keep each child's forward mapping until its descendants have been released: the
        // parent key replaces a recursive call's return address. Reverse sets are consumed
        // under the graph lock, so no owner lookup can observe the temporary mismatch.
        let mut query = database_key;
        loop {
            if let Some(child) = self
                .transferred_dependents
                .get_mut(&query)
                .and_then(SmallSet::pop)
            {
                #[cfg(all(test, not(feature = "shuttle")))]
                {
                    self.release_counts.children += 1;
                }
                self.unblock_runtimes_blocked_on(child, wait_result);
                query = child;
                continue;
            }

            self.transferred_dependents.remove(&query);
            #[cfg(all(test, not(feature = "shuttle")))]
            {
                self.release_counts.queries += 1;
            }
            if query == database_key {
                break;
            }
            let (_thread, parent) = self
                .transferred
                .remove(&query)
                .expect("transferred child retains its parent until release");
            #[cfg(all(test, not(feature = "shuttle")))]
            Self::transfer_test_mapping(Kind::MappingRemoved, query, _thread, parent);
            query = parent;
        }
    }

    pub(super) fn undo_transfer_lock(&mut self, database_key: DatabaseKeyIndex) {
        if let Some((_thread, owner)) = self.transferred.remove(&database_key) {
            #[cfg(all(test, not(feature = "shuttle")))]
            Self::transfer_test_mapping(Kind::Undo, database_key, _thread, owner);
            self.transferred_dependents
                .get_mut(&owner)
                .unwrap()
                .remove(&database_key);
        }
    }

    /// Recursively resolves the thread id that currently owns the lock for `database_key`.
    ///
    /// Returns `None` if `database_key` hasn't (or has since then been released) transferred its lock
    /// and the thread id must be looked up in the `SyncTable` instead.
    pub(super) fn thread_id_of_transferred_query(
        &self,
        database_key: DatabaseKeyIndex,
        skip_over: Option<DatabaseKeyIndex>,
    ) -> Option<ThreadId> {
        let &(mut resolved_thread, owner) = self.transferred.get(&database_key)?;

        let mut current_owner = owner;

        while let Some(&(next_thread, next_key)) = self.transferred.get(&current_owner) {
            #[cfg(all(test, not(feature = "shuttle")))]
            tests::count_work(tests::WorkKind::Chain, 1);

            current_owner = next_key;

            // Ignore the `skip_over` key. E.g. if we have `a -> b -> c` and we want to resolve `a` but are transferring `b` to `c`, then
            // we don't want to resolve `a` to the owner of `c`. But for `a -> c -> b`, we want resolve `a` to the owner of `c` and not `b`
            // (because `b` will be owned by `a`).
            if Some(next_key) == skip_over {
                continue;
            }

            resolved_thread = next_thread;
        }

        Some(resolved_thread)
    }

    /// Modifies the graph so that the lock on `query` (currently owned by `current_thread`) is
    /// transferred to `new_owner` (which is owned by `new_owner_id`).
    ///
    /// Note, this function will block if `new_owner` runs on a different thread, unless `new_owner` is blocked
    /// on current thread after transferring the query ownership.
    ///
    /// Returns `true` if the transfer blocked on `new_owner` (in which case it might be necessary to refetch any previously computed memos).
    pub(super) fn transfer_lock(
        me: MutexGuard<Self>,
        query: DatabaseKeyIndex,
        current_thread: ThreadId,
        new_owner: DatabaseKeyIndex,
        new_owner_id: SyncOwner,
        guard: SyncGuard,
    ) -> bool {
        Self::transfer_lock_with_cut(
            me,
            query,
            current_thread,
            new_owner,
            new_owner_id,
            guard,
            None,
        )
    }

    fn transfer_lock_with_cut(
        mut me: MutexGuard<Self>,
        query: DatabaseKeyIndex,
        current_thread: ThreadId,
        new_owner: DatabaseKeyIndex,
        new_owner_id: SyncOwner,
        guard: SyncGuard,
        mandatory_cut: Option<(DatabaseKeyIndex, usize)>,
    ) -> bool {
        let dg = &mut *me;
        let new_owner_thread = match new_owner_id {
            SyncOwner::Thread(thread) => thread,
            SyncOwner::Transferred => {
                // Skip over `query` to skip over any existing mapping from `new_owner` to `query` that may
                // exist from previous transfers.
                dg.thread_id_of_transferred_query(new_owner, Some(query))
                    .expect("new owner should be blocked on `query`")
            }
        };

        debug_assert!(
            new_owner_thread == current_thread || dg.depends_on(new_owner_thread, current_thread),
            "new owner {new_owner:?} ({new_owner_thread:?}) must be blocked on {query:?} ({current_thread:?})"
        );

        let unchanged_mapping = dg.transferred.get(&query) == Some(&(new_owner_thread, new_owner));
        let thread_changed = if unchanged_mapping {
            if mandatory_cut.is_none() {
                return false;
            }
            true
        } else {
            match dg.transferred.entry(query) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    // Transfer `c -> b` and there's no existing entry for `c`.
                    entry.insert((new_owner_thread, new_owner));
                    current_thread != new_owner_thread
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    // If we transfer to the same owner as before, return immediately as this is a no-op.
                    if entry.get() == &(new_owner_thread, new_owner) {
                        return false;
                    }

                    // `Transfer `c -> b` after a previous `c -> d` mapping.
                    // Update the owner and remove the query from the old owner's dependents.
                    let &(old_owner_thread, old_owner) = entry.get();

                    // For the example below, remove `d` from `b`'s dependents.`
                    dg.transferred_dependents
                        .get_mut(&old_owner)
                        .unwrap()
                        .remove(&query);

                    entry.insert((new_owner_thread, new_owner));

                    // If we have `c -> a -> d` and we now insert a mapping `d -> c`, rewrite the mapping to
                    // `d -> c -> a` to avoid cycles.
                    //
                    // Or, starting with `e -> c -> a -> d -> b` insert `d -> c`. We need to rewrite the tree to
                    // ```
                    // e -> c -> a -> b
                    // d /
                    // ```
                    //
                    // A cycle between transfers can occur when a later iteration has a different outer most query than
                    // a previous iteration. The second iteration then hits `cycle_initial` for a different head, (e.g. for `c` where it previously was `d`).
                    let mut last_segment = dg.transferred.entry(new_owner);

                    while let std::collections::hash_map::Entry::Occupied(mut entry) = last_segment
                    {
                        #[cfg(all(test, not(feature = "shuttle")))]
                        tests::count_work(tests::WorkKind::Chain, 1);
                        let source = *entry.key();
                        let next_target = entry.get().1;

                        // If it's `a -> d`, remove `a -> d` and insert an edge from `a -> b`
                        if next_target == query {
                            tracing::trace!(
                                "Remap edge {source:?} -> {next_target:?} to {source:?} -> {old_owner:?} to prevent a cycle",
                            );

                            // Remove `a` from the dependents of `d` and remove the mapping from `a -> d`.
                            dg.transferred_dependents
                                .get_mut(&query)
                                .unwrap()
                                .remove(&source);

                            // if the old mapping was `c -> d` and we now insert `d -> c`, remove `c -> d`
                            if old_owner == new_owner {
                                entry.remove();
                                #[cfg(all(test, not(feature = "shuttle")))]
                                Self::transfer_test_mapping(
                                    Kind::MappingRemoved,
                                    source,
                                    old_owner_thread,
                                    next_target,
                                );
                            } else {
                                // otherwise (when `d` pointed to some other query, e.g. `b` in the example),
                                // add an edge from `a` to `b`
                                entry.insert((old_owner_thread, old_owner));
                                dg.transferred_dependents
                                    .get_mut(&old_owner)
                                    .unwrap()
                                    .push(source);
                                #[cfg(all(test, not(feature = "shuttle")))]
                                Self::transfer_test_mapping(
                                    Kind::Mapping,
                                    source,
                                    old_owner_thread,
                                    old_owner,
                                );
                            }

                            break;
                        }

                        last_segment = dg.transferred.entry(next_target);
                    }

                    // We simply assume here that the thread has changed because we'd have to walk the entire
                    // transferred chaine of `old_owner` to know if the thread has changed. This won't save us much
                    // compared to just updating all dependent threads.
                    true
                }
            }
        };

        // Register `c` as a dependent of `b`.
        if !unchanged_mapping {
            let all_dependents = dg.transferred_dependents.entry(new_owner).or_default();
            debug_assert!(!all_dependents.contains(&new_owner));
            all_dependents.push(query);
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        Self::transfer_test_mapping(Kind::Mapping, query, new_owner_thread, new_owner);

        if thread_changed {
            tracing::debug!("Unblocking new owner of transfer target {new_owner:?}");
            if let Some(cut) = mandatory_cut {
                dg.unblock_transfer_waiter(cut);
            } else {
                dg.unblock_transfer_target(query, new_owner_thread);
            }
            dg.update_transferred_edges(query, new_owner_thread);

            // Block on the new owner, unless new owner is blocked on this query.
            // This is necessary to avoid a race between `fetch` completing and `provisional_retry` blocking on the
            // first cycle head.
            if current_thread != new_owner_thread
                && !dg.depends_on(new_owner_thread, current_thread)
            {
                crate::tracing::debug!(
                    "block_on: thread {current_thread:?} is blocking on {new_owner:?} in thread {new_owner_thread:?}",
                );
                #[cfg(all(test, not(feature = "shuttle")))]
                Self::transfer_test_mapping(
                    Kind::TransferWaitBegin,
                    query,
                    new_owner_thread,
                    new_owner,
                );
                let _result =
                    Self::block_on(me, current_thread, new_owner, new_owner_thread, guard);
                #[cfg(all(test, not(feature = "shuttle")))]
                {
                    let mut event = Event::new(Kind::TransferWaitEnd).key(query).decision(true);
                    event.other_key = Some(new_owner);
                    event.peer = Some(new_owner_thread);
                    event.wait = Some(_result);
                    transfer_trace::record(event);
                }
                return true;
            }
        }

        false
    }

    /// Finds the one query in the dependents of the `source_query` (the one that is transferred to a new owner)
    /// on which the `new_owner_id` thread blocks on and unblocks it, to ensure progress.
    fn unblock_transfer_target(&mut self, source_query: DatabaseKeyIndex, new_owner_id: ThreadId) {
        if let Some(cut) = self.transfer_waiter(source_query, new_owner_id, None) {
            self.unblock_transfer_waiter(cut);
        }
    }

    fn transfer_waiter(
        &self,
        source_query: DatabaseKeyIndex,
        new_owner_id: ThreadId,
        excluded: Option<DatabaseKeyIndex>,
    ) -> Option<(DatabaseKeyIndex, usize)> {
        // Retain the query and its waiter index until the immutable tree walk has ended.
        let mut nearest: Option<(DatabaseKeyIndex, usize, ThreadId)> = None;
        for query in
            TransferredQueries::excluding(&self.transferred_dependents, source_query, excluded)
        {
            if let Some(blocked_threads) = self.query_dependents.get(&query) {
                for (i, id) in blocked_threads.iter().copied().enumerate() {
                    #[cfg(all(test, not(feature = "shuttle")))]
                    tests::count_work(tests::WorkKind::Waiter, 1);
                    if (id == new_owner_id || self.edges.depends_on(new_owner_id, id))
                        && nearest
                            .is_none_or(|(_, _, previous)| self.edges.depends_on(id, previous))
                    {
                        nearest = Some((query, i, id));
                    }
                }
            }
        }
        nearest.map(|(query, index, _)| (query, index))
    }

    fn unblock_transfer_waiter(
        &mut self,
        (query, query_dependents_index): (DatabaseKeyIndex, usize),
    ) {
        let blocked_threads = self.query_dependents.get_mut(&query).unwrap();

        let thread_id = blocked_threads.swap_remove(query_dependents_index);
        if blocked_threads.is_empty() {
            self.query_dependents.remove(&query);
        }

        self.unblock_runtime(thread_id, WaitResult::Completed);
    }

    fn update_transferred_edges(&mut self, query: DatabaseKeyIndex, new_owner_thread: ThreadId) {
        let Self {
            edges,
            query_dependents,
            transferred_dependents,
            ..
        } = self;
        for query in TransferredQueries::new(transferred_dependents, query) {
            tracing::trace!("update_transferred_edges({query:?}");
            if let Some(dependents) = query_dependents.get(&query) {
                for dependent in dependents.iter() {
                    let edge = edges.get_mut(dependent).unwrap();

                    tracing::trace!(
                        "Rewrite edge from {:?} to {new_owner_thread:?}",
                        edge.blocked_on_id
                    );
                    edge.blocked_on_id = new_owner_thread;
                    #[cfg(all(test, not(feature = "shuttle")))]
                    {
                        let mut event = Event::new(Kind::EdgeRemap).key(query);
                        event.from = Some(*dependent);
                        event.peer = Some(new_owner_thread);
                        transfer_trace::record(event);
                    }
                    debug_assert!(
                        !edges.depends_on(new_owner_thread, *dependent),
                        "Circular reference between blocked edges: {:#?}",
                        edges
                    );
                }
            }
        }
    }
}

/// Visits transferred queries in the same preorder as their reverse sets. Iterator frames
/// borrow the existing sets and have flat destruction, including an early search return.
struct TransferredQueries<'a> {
    dependents: &'a TransferredDependents,
    root: Option<DatabaseKeyIndex>,
    ancestors: Vec<std::slice::Iter<'a, DatabaseKeyIndex>>,
    excluded: Option<DatabaseKeyIndex>,
}

impl<'a> TransferredQueries<'a> {
    fn new(dependents: &'a TransferredDependents, root: DatabaseKeyIndex) -> Self {
        Self::excluding(dependents, root, None)
    }

    fn excluding(
        dependents: &'a TransferredDependents,
        root: DatabaseKeyIndex,
        excluded: Option<DatabaseKeyIndex>,
    ) -> Self {
        #[cfg(all(test, not(feature = "shuttle")))]
        tests::count_work(tests::WorkKind::Cursor, 1);
        Self {
            dependents,
            root: Some(root),
            ancestors: Vec::new(),
            excluded,
        }
    }
}

impl Iterator for TransferredQueries<'_> {
    type Item = DatabaseKeyIndex;

    fn next(&mut self) -> Option<Self::Item> {
        let query = self.root.take().or_else(|| {
            loop {
                #[cfg(all(test, not(feature = "shuttle")))]
                tests::count_work(tests::WorkKind::Tree, 1);
                let children = self.ancestors.last_mut()?;
                if let Some(child) = children.next() {
                    if Some(*child) != self.excluded {
                        break Some(*child);
                    }
                    continue;
                }
                self.ancestors.pop();
            }
        })?;
        #[cfg(all(test, not(feature = "shuttle")))]
        tests::count_work(tests::WorkKind::Tree, 1);
        if let Some(children) = self.dependents.get(&query) {
            #[cfg(all(test, not(feature = "shuttle")))]
            if self.ancestors.len() == self.ancestors.capacity() {
                tests::count_work(tests::WorkKind::CursorGrowth, 1);
            }
            self.ancestors.push(children.iter());
        }
        Some(query)
    }
}

#[derive(Debug, Default)]
struct Edges(FxHashMap<ThreadId, edge::Edge>);

impl Edges {
    fn depends_on(&self, from_id: ThreadId, to_id: ThreadId) -> bool {
        let mut p = from_id;
        while let Some(q) = self.0.get(&p).map(|edge| edge.blocked_on_id) {
            #[cfg(all(test, not(feature = "shuttle")))]
            tests::count_work(tests::WorkKind::Dependency, 1);

            if q == to_id {
                return true;
            }

            p = q;
        }
        p == to_id
    }

    fn get_mut(&mut self, id: &ThreadId) -> Option<&mut edge::Edge> {
        self.0.get_mut(id)
    }

    fn contains_key(&self, id: &ThreadId) -> bool {
        self.0.contains_key(id)
    }

    fn insert(&mut self, id: ThreadId, edge: edge::Edge) {
        self.0.insert(id, edge);
    }

    fn remove(&mut self, id: &ThreadId) -> Option<edge::Edge> {
        self.0.remove(id)
    }
}

#[derive(Debug)]
struct SmallSet<T, const N: usize>(SmallVec<[T; N]>);

impl<T, const N: usize> SmallSet<T, N>
where
    T: PartialEq,
{
    const fn new() -> Self {
        Self(SmallVec::new_const())
    }

    fn push(&mut self, value: T) {
        debug_assert!(!self.contains(&value));

        self.0.push(value);
    }

    fn contains(&self, value: &T) -> bool {
        #[cfg(all(test, not(feature = "shuttle")))]
        return self.0.iter().any(|candidate| {
            tests::count_work(tests::WorkKind::Membership, 1);
            candidate == value
        });
        #[cfg(not(all(test, not(feature = "shuttle"))))]
        self.0.contains(value)
    }

    fn pop(&mut self) -> Option<T> {
        self.0.pop()
    }

    fn remove(&mut self, value: &T) -> bool {
        if let Some(index) = self.0.iter().position(|x| {
            #[cfg(all(test, not(feature = "shuttle")))]
            tests::count_work(tests::WorkKind::Membership, 1);
            x == value
        }) {
            self.0.swap_remove(index);
            true
        } else {
            false
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }
}

impl<T, const N: usize> IntoIterator for SmallSet<T, N> {
    type Item = T;
    type IntoIter = smallvec::IntoIter<[T; N]>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallSet<T, N>
where
    T: PartialEq,
{
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, const N: usize> Default for SmallSet<T, N>
where
    T: PartialEq,
{
    fn default() -> Self {
        Self::new()
    }
}

mod edge {
    use crate::sync::thread::ThreadId;
    use crate::sync::{Condvar, MutexGuard};

    use std::pin::Pin;

    #[derive(Default, Debug)]
    pub(super) struct EdgeCondvar {
        condvar: Condvar,
        _phantom_pin: std::marker::PhantomPinned,
    }

    impl EdgeCondvar {
        #[inline]
        pub(super) fn wait<'a, T>(&self, mutex_guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
            self.condvar.wait(mutex_guard)
        }
    }

    #[derive(Debug)]
    pub(super) struct Edge {
        pub(super) blocked_on_id: ThreadId,

        /// Signalled whenever a query with dependents completes.
        /// Allows those dependents to check if they are ready to unblock.
        /// `condvar: unsafe<'stack_frame> Pin<&'stack_frame Condvar>`
        condvar: Pin<&'static EdgeCondvar>,
    }

    impl Edge {
        /// # SAFETY
        ///
        /// The caller must ensure that the [`EdgeCondvar`] is kept alive until the [`Edge`] is dropped.
        pub(super) unsafe fn new(blocked_on_id: ThreadId, condvar: Pin<&EdgeCondvar>) -> Self {
            Self {
                blocked_on_id,
                // SAFETY: The caller is responsible for ensuring that the `EdgeCondvar` outlives the `Edge`.
                condvar: unsafe {
                    std::mem::transmute::<Pin<&EdgeCondvar>, Pin<&'static EdgeCondvar>>(condvar)
                },
            }
        }

        #[inline]
        pub(super) fn notify(self) {
            self.condvar.condvar.notify_one();
        }
    }
}
