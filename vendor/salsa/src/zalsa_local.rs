use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::{RefCell, UnsafeCell};
use std::fmt;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::panic::UnwindSafe;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use rustc_hash::FxHashMap;
use thin_vec::ThinVec;

#[cfg(feature = "accumulator")]
use crate::accumulator::{
    Accumulator,
    accumulated_map::{AccumulatedMap, AtomicInputAccumulatedValues},
};
use crate::active_query::{CompletedQuery, DetachedInputOutputs, QueryCompletion, QueryStack};
use crate::attempt_probe::AttemptSupport;
use crate::cycle::{
    AtomicIterationStamp, CycleHeads, IterationStamp, PreparedIterationStore, empty_cycle_heads,
};
use crate::durability::Durability;
use crate::key::DatabaseKeyIndex;
use crate::runtime::Stamp;
use crate::sync::atomic::AtomicBool;
use crate::table::{PageIndex, Slot, Table};
use crate::tracked_struct::{Disambiguator, Identity, IdentityHash};
use crate::zalsa::{IngredientIndex, Zalsa};
use crate::{Cancelled, Id, Revision};

pub(crate) mod dependency_read;

pub(crate) struct ActiveQueryCycle {
    pub(crate) key: DatabaseKeyIndex,
    pub(crate) stamp: Stamp,
    pub(crate) heads: CycleHeads,
    pub(crate) support: Option<AttemptSupport>,
}

/// State that is specific to a single execution thread.
///
/// Internally, this type uses ref-cells.
///
/// **Note also that all mutations to the database handle (and hence
/// to the local-state) must be undone during unwinding.**
pub struct ZalsaLocal {
    /// Vector of active queries.
    ///
    /// Unwinding note: pushes onto this vector must be popped -- even
    /// during unwinding.
    query_stack: RefCell<QueryStack>,

    /// Stores the most recent page for a given ingredient.
    /// This is thread-local to avoid contention.
    most_recent_pages: UnsafeCell<FxHashMap<IngredientIndex, PageIndex>>,

    cancelled: CancellationToken,
}

/// A cancellation token that can be used to cancel a query computation for a specific local `Database`.
#[derive(Default, Clone, Debug)]
pub struct CancellationToken(Arc<AtomicU8>);

impl CancellationToken {
    const CANCELLED_MASK: u8 = 0b01;
    const DISABLED_MASK: u8 = 0b10;

    /// Inform the database to cancel the current query computation.
    pub fn cancel(&self) {
        self.0.fetch_or(Self::CANCELLED_MASK, Ordering::Relaxed);
    }

    /// Check if the query computation has been requested to be cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed) & Self::CANCELLED_MASK != 0
    }

    #[inline]
    fn set_cancellation_disabled(&self, disabled: bool) -> bool {
        let previous_disabled_bit = if disabled {
            self.0.fetch_or(Self::DISABLED_MASK, Ordering::Relaxed)
        } else {
            self.0.fetch_and(!Self::DISABLED_MASK, Ordering::Relaxed)
        };
        previous_disabled_bit & Self::DISABLED_MASK != 0
    }

    fn should_trigger_local_cancellation(&self) -> bool {
        self.0.load(Ordering::Relaxed) == Self::CANCELLED_MASK
    }

    fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

impl ZalsaLocal {
    pub(crate) fn mark_attempt_incomplete(&self, zalsa: &Zalsa) {
        let support = crate::attempt_probe::current().expect("incomplete query outside attempt");
        self.report_attempt_read(zalsa, &support, true);
    }

    pub(crate) fn report_attempt_read(
        &self,
        zalsa: &Zalsa,
        support: &AttemptSupport,
        incomplete: bool,
    ) {
        // SAFETY: This callback does not access the query stack recursively.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                if let Some(query) = stack.last() {
                    query.assert_attempt_support_allowed(zalsa, support, incomplete);
                }
                if incomplete {
                    crate::attempt_probe::observe_incomplete(support);
                }
                if let Some(query) = stack.last_mut() {
                    query.add_attempt_support(zalsa, support, incomplete);
                }
            });
        }
    }

    pub(crate) fn new() -> Self {
        ZalsaLocal {
            query_stack: RefCell::new(QueryStack::default()),
            most_recent_pages: UnsafeCell::new(FxHashMap::default()),
            cancelled: CancellationToken::default(),
        }
    }

    pub(crate) fn record_unfilled_pages(&mut self, table: &Table) {
        let most_recent_pages = self.most_recent_pages.get_mut();
        most_recent_pages
            .drain()
            .for_each(|(ingredient, page)| table.record_unfilled_page(ingredient, page));
    }

    /// Allocate a new id in `table` for the given ingredient
    /// storing `value`. Remembers the most recent page from this
    /// thread and attempts to reuse it.
    pub(crate) fn allocate<'db, T: Slot>(
        &self,
        zalsa: &'db Zalsa,
        ingredient: IngredientIndex,
        mut value: impl FnOnce(Id) -> T,
    ) -> (Id, &'db T) {
        // SAFETY: `ZalsaLocal` is `!Sync`, and we never expose a reference to this field,
        // so we have exclusive access.
        let most_recent_pages = unsafe { &mut *self.most_recent_pages.get() };

        // Fast-path, we already have an unfilled page available.
        if let Some(&page) = most_recent_pages.get(&ingredient) {
            let page_ref = zalsa.table().page::<T>(page);

            // SAFETY: `ZalsaLocal` is `!Sync`, and we only insert a page into `most_recent_pages`
            // if it was allocated by our thread, so we are the unique writer.
            match unsafe { page_ref.allocate(page, value) } {
                Ok((id, value)) => return (id, value),
                Err(v) => value = v,
            }
        }

        self.allocate_cold(zalsa, ingredient, value)
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn allocate_cold<'db, T: Slot>(
        &self,
        zalsa: &'db Zalsa,
        ingredient: IngredientIndex,
        mut value: impl FnOnce(Id) -> T,
    ) -> (Id, &'db T) {
        let memo_types = || {
            zalsa
                .lookup_ingredient(ingredient)
                .memo_table_types()
                .clone()
        };

        // SAFETY: `ZalsaLocal` is `!Sync`, and we never expose a reference to this field,
        // so we have exclusive access.
        let most_recent_pages = unsafe { &mut *self.most_recent_pages.get() };

        // Find the most recent page, pushing a page if needed
        let mut page = *most_recent_pages.entry(ingredient).or_insert_with(|| {
            zalsa
                .table()
                .fetch_or_push_page::<T>(ingredient, memo_types)
        });

        loop {
            // Try to allocate an entry on that page
            let page_ref = zalsa.table().page::<T>(page);

            // SAFETY: `ZalsaLocal` is `!Sync`, and we only insert a page into `most_recent_pages`
            // if it was allocated by our thread, so we are the unique writer.
            match unsafe { page_ref.allocate(page, value) } {
                // If successful, return
                Ok((id, value)) => return (id, value),

                // Otherwise, create a new page and try again.
                //
                // Note that we could try fetching a page again, but as we just filled one up
                // it is unlikely that there is a non-full one available.
                Err(v) => {
                    value = v;
                    page = zalsa.table().push_page::<T>(ingredient, memo_types());
                    most_recent_pages.insert(ingredient, page);
                }
            }
        }
    }

    #[inline]
    pub(crate) fn push_query(&self, database_key_index: DatabaseKeyIndex) -> ActiveQueryGuard<'_> {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                stack.push_new_query(database_key_index);

                ActiveQueryGuard {
                    local_state: self,
                    database_key_index,
                    #[cfg(debug_assertions)]
                    push_len: stack.len(),
                }
            })
        }
    }

    /// Executes a closure within the context of the current active query stacks (mutable).
    ///
    /// # Safety
    ///
    /// The closure cannot access the query stack reentrantly, whether mutable or immutable.
    #[inline(always)]
    pub(crate) unsafe fn with_query_stack_unchecked_mut<R>(
        &self,
        f: impl UnwindSafe + FnOnce(&mut QueryStack) -> R,
    ) -> R {
        // SAFETY: The caller guarantees that the query stack will not be accessed reentrantly.
        // Additionally, `ZalsaLocal` is `!Sync`, and we never expose a reference to the query
        // stack except through this method, so we have exclusive access.
        unsafe { f(&mut self.query_stack.try_borrow_mut().unwrap_unchecked()) }
    }

    /// Executes a closure within the context of the current active query stacks.
    ///
    /// # Safety
    ///
    /// No mutable references to the query stack can exist while the closure is executed.
    #[inline(always)]
    pub(crate) unsafe fn with_query_stack_unchecked<R>(
        &self,
        f: impl UnwindSafe + FnOnce(&QueryStack) -> R,
    ) -> R {
        // SAFETY: The caller guarantees that the query stack will not being accessed mutably.
        // Additionally, `ZalsaLocal` is `!Sync`, and we never expose a reference to the query
        // stack except through this method, so we have exclusive access.
        unsafe { f(&self.query_stack.try_borrow().unwrap_unchecked()) }
    }

    #[inline(always)]
    pub(crate) fn try_with_query_stack<R>(
        &self,
        f: impl UnwindSafe + FnOnce(&QueryStack) -> R,
    ) -> Option<R> {
        self.query_stack
            .try_borrow()
            .ok()
            .as_ref()
            .map(|stack| f(stack))
    }

    /// Retains semantic frames while the executor prepares their structural dependencies.
    /// Query guards remain owned by suspended tasks and must not run until this guard drops.
    pub(crate) fn park_for_structural_preparation(
        &self,
    ) -> Result<ParkedQueryStack<'_>, &'static str> {
        let stack = std::mem::take(
            &mut *self
                .query_stack
                .try_borrow_mut()
                .map_err(|_| "structural preparation has a borrowed query stack")?,
        );
        let cancellation_was_disabled = self.set_cancellation_disabled(false);
        Ok(ParkedQueryStack {
            local: self,
            stack,
            cancellation_was_disabled,
        })
    }

    /// Returns the index of the active query along with its *current* durability/changed-at
    /// information. As the query continues to execute, naturally, that information may change.
    pub(crate) fn active_query(&self) -> Option<(DatabaseKeyIndex, Stamp)> {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked(|stack| {
                stack
                    .last()
                    .map(|active_query| (active_query.database_key_index, active_query.stamp()))
            })
        }
    }

    /// Returns the active query, its current dependencies, and any provisional cycle results it
    /// depends on.
    pub(crate) fn active_query_with_cycle_heads(&self) -> Option<ActiveQueryCycle> {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked(|stack| {
                stack.last().map(|active_query| {
                    ActiveQueryCycle {
                        key: active_query.database_key_index,
                        stamp: active_query.stamp(),
                        heads: active_query.cycle_heads().clone(),
                        support: active_query.attempt_support().cloned(),
                    }
                })
            })
        }
    }

    /// Add an output to the current query's list of dependencies
    ///
    /// Returns `Err` if not in a query.
    #[cfg(feature = "accumulator")]
    pub(crate) fn accumulate<A: Accumulator>(
        &self,
        index: IngredientIndex,
        value: A,
    ) -> Result<(), ()> {
        crate::attempt_probe::assert_output_allowed(self);
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                if let Some(top_query) = stack.last_mut() {
                    top_query.accumulate(index, value);
                    Ok(())
                } else {
                    Err(())
                }
            })
        }
    }

    /// Add an output to the current query's list of dependencies, returning whether it was new.
    pub(crate) fn add_output(&self, entity: DatabaseKeyIndex) -> bool {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                stack
                    .last_mut()
                    .is_some_and(|top_query| top_query.add_output(entity))
            })
        }
    }

    /// Check whether `entity` is a tracked struct that was created by the currently active query (if any)
    pub(crate) fn is_tracked_struct_of_active_query(&self, entity: DatabaseKeyIndex) -> bool {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                stack
                    .last_mut()
                    .is_some_and(|top_query| top_query.tracked_struct_ids().is_active(entity))
            })
        }
    }

    /// Register that currently active query reads the given input
    #[inline(always)]
    pub(crate) fn report_tracked_read(
        &self,
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
        cycle_heads: &CycleHeads,
        #[cfg(feature = "accumulator")] has_accumulated: bool,
        #[cfg(feature = "accumulator")] accumulated_inputs: &AtomicInputAccumulatedValues,
    ) {
        crate::tracing::debug!(
            "report_tracked_read(input={:?}, durability={:?}, changed_at={:?})",
            input,
            durability,
            changed_at
        );

        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                if let Some(top_query) = stack.last_mut() {
                    top_query.add_read(
                        input,
                        durability,
                        changed_at,
                        cycle_heads,
                        #[cfg(feature = "accumulator")]
                        has_accumulated,
                        #[cfg(feature = "accumulator")]
                        accumulated_inputs,
                    );
                }
            })
        }
    }

    /// Register that currently active query reads the given input
    #[inline(always)]
    pub(crate) fn report_tracked_read_simple(
        &self,
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
    ) {
        crate::tracing::debug!(
            "report_tracked_read(input={:?}, durability={:?}, changed_at={:?})",
            input,
            durability,
            changed_at
        );

        self.apply_dependency_read(&dependency_read::DependencyRead::Simple {
            input,
            durability,
            changed_at,
        });
    }

    /// Update the active query's changed revision without recording a dependency edge.
    #[inline(always)]
    pub(crate) fn report_tracked_read_revision(&self, changed_at: Revision) {
        self.apply_dependency_read(&dependency_read::DependencyRead::Revision(changed_at));
    }

    /// Register that the current query read an untracked value
    ///
    /// # Parameters
    ///
    /// * `current_revision`, the current revision
    #[inline(always)]
    pub(crate) fn report_untracked_read(&self, current_revision: Revision) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                if let Some(top_query) = stack.last_mut() {
                    top_query.add_untracked_read(current_revision);
                }
            })
        }
    }

    /// Called when the active queries creates an index from the
    /// entity table with the index `entity_index`. Has the following effects:
    ///
    /// * Add a query read on `DatabaseKeyIndex::for_table(entity_index)`
    /// * Identify a unique disambiguator for the hash within the current query,
    ///   adding the hash to the current query's disambiguator table.
    /// * Returns a tuple of:
    ///   * the id of the current query
    ///   * the current dependencies (durability, changed_at) of current query
    ///   * the disambiguator index
    #[track_caller]
    pub(crate) fn disambiguate(&self, key: IdentityHash) -> (Stamp, Disambiguator) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                let top_query = stack.last_mut().expect(
                    "cannot create a tracked struct disambiguator outside of a tracked function",
                );
                let disambiguator = top_query.disambiguate(key);
                (top_query.stamp(), disambiguator)
            })
        }
    }

    #[track_caller]
    pub(crate) fn tracked_struct_id(&self, identity: &Identity) -> Option<Id> {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                let top_query = stack
                    .last_mut()
                    .expect("cannot create a tracked struct ID outside of a tracked function");
                top_query.tracked_struct_ids_mut().reuse(identity)
            })
        }
    }

    #[track_caller]
    pub(crate) fn store_tracked_struct_id(&self, identity: Identity, id: Id) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.with_query_stack_unchecked_mut(|stack| {
                let top_query = stack
                    .last_mut()
                    .expect("cannot store a tracked struct ID outside of a tracked function");
                top_query.tracked_struct_ids_mut().insert(identity, id);
            })
        }
    }

    #[inline]
    pub(crate) fn cancellation_token(&self) -> CancellationToken {
        self.cancelled.clone()
    }

    #[inline]
    pub(crate) fn uncancel(&self) {
        self.cancelled.reset();
    }

    #[inline]
    pub fn should_trigger_local_cancellation(&self) -> bool {
        self.cancelled.should_trigger_local_cancellation()
    }

    #[cold]
    pub(crate) fn unwind_pending_write(&self) {
        Cancelled::PendingWrite.throw();
    }

    #[cold]
    pub(crate) fn unwind_cancelled(&self) {
        Cancelled::Local.throw();
    }

    #[inline]
    pub(crate) fn set_cancellation_disabled(&self, was_disabled: bool) -> bool {
        self.cancelled.set_cancellation_disabled(was_disabled)
    }
}

pub(crate) struct ParkedQueryStack<'a> {
    local: &'a ZalsaLocal,
    stack: QueryStack,
    cancellation_was_disabled: bool,
}

impl ParkedQueryStack<'_> {
    pub(crate) fn check_finished(&self) -> Result<(), &'static str> {
        if self.local.try_with_query_stack(|stack| stack.is_empty()) != Some(true) {
            return Err("structural preparation retained an active query");
        }
        Ok(())
    }
}

impl Drop for ParkedQueryStack<'_> {
    fn drop(&mut self) {
        std::mem::swap(&mut *self.local.query_stack.borrow_mut(), &mut self.stack);
        // Cycle evaluation can mask cancellation. Preserve the request bit while restoring only
        // that mask, so structural work cannot consume cancellation intended for its parent.
        self.local
            .set_cancellation_disabled(self.cancellation_was_disabled);
    }
}

// Okay to implement as `ZalsaLocal`` is !Sync
// - `most_recent_pages` can't observe broken states as we cannot panic such that we enter an
//   inconsistent state
// - neither can `query_stack` as we require the closures accessing it to be `UnwindSafe`
impl std::panic::RefUnwindSafe for ZalsaLocal {}

/// Summarizes "all the inputs that a query used" and "all the outputs it has written to".
#[derive(Debug)]
// #[derive(Clone)] cloning this is expensive, so we don't derive
pub(crate) struct QueryRevisions {
    /// The most revision in which some input changed.
    pub(crate) changed_at: Revision,

    /// Minimum durability of the inputs to this query.
    pub(crate) durability: Durability,

    /// The query origin and optional data for tracked structs, cycles, and accumulators.
    pub(crate) origin_and_extra: OriginAndExtra,

    /// [`InputAccumulatedValues::Empty`] if any input read during the query's execution
    /// has any direct or indirect accumulated values.
    ///
    /// Note that this field could be in `QueryRevisionsExtra` as it is only relevant
    /// for accumulators, but we get it for free anyways due to padding.
    #[cfg(feature = "accumulator")]
    pub(super) accumulated_inputs: AtomicInputAccumulatedValues,

    /// Are the `cycle_heads` verified to not be provisional anymore?
    ///
    /// Note that this field could be in `QueryRevisionsExtra` as it is only
    /// relevant for queries that participate in a cycle, but we get it for
    /// free anyways due to padding.
    pub(super) verified_final: AtomicBool,
}

impl QueryRevisions {
    pub(crate) fn prepare_iteration_count(
        &self,
        key: DatabaseKeyIndex,
        next: IterationStamp,
    ) -> Result<Option<PreparedIterationUpdate<'_>>, &'static str> {
        let Some(extra) = self.origin_and_extra.extra() else {
            return Ok(None);
        };
        if extra.iteration.load() > next {
            return Err("cycle iteration moved backwards");
        }
        Ok(Some(PreparedIterationUpdate {
            iteration: extra.iteration.prepare_store(next)?,
            own_head: extra.cycle_heads.prepare_iteration_store(key, next)?,
        }))
    }
    pub(crate) fn attempt_support(&self) -> Option<&AttemptSupport> {
        self.origin_and_extra
            .extra()
            .and_then(|extra| extra.attempt_support.as_ref())
    }

    pub(crate) fn execution_revision(&self) -> Option<Revision> {
        self.origin_and_extra
            .extra()
            .and_then(|extra| extra.execution_revision)
    }

    /// Records fresh execution before publication, without allocating metadata where none exists.
    pub(crate) fn record_execution_revision(&mut self, revision: Revision) {
        if let Some(extra) = self.origin_and_extra.extra_mut() {
            extra.execution_revision = Some(revision);
        }
    }

    pub(crate) fn has_incomplete_attempt(&self) -> bool {
        self.attempt_support().is_some_and(|support| {
            support.incomplete(
                !self
                    .verified_final
                    .load(crate::sync::atomic::Ordering::Acquire),
            )
        })
    }

    pub(crate) fn finish_attempt_incomplete(&mut self, support: &AttemptSupport) {
        assert!(
            support.reason().is_some(),
            "incomplete completion has no refusal reason"
        );
        assert!(
            self.tracked_struct_ids().is_empty(),
            "incomplete query owns tracked structs"
        );
        assert!(
            self.origin().outputs().next().is_none(),
            "incomplete query owns outputs"
        );
        #[cfg(feature = "accumulator")]
        assert!(
            self.accumulated().is_none(),
            "incomplete query owns accumulators"
        );

        // Completion callbacks run after the active frame has been popped. Preserve the
        // refusal explicitly: clearing cycle heads must not turn its typed return into
        // an independent completed value when the owner leaves the attempt.
        let extra = self.origin_and_extra.get_or_insert_extra();
        if let Some(previous) = &extra.attempt_support {
            assert!(
                previous.same_owner(support),
                "completion mixed attempt owners"
            );
        }
        let mut support = support.clone();
        support.make_incomplete();
        extra.attempt_support = Some(support);
        extra.cycle_heads = CycleHeads::default();
        extra.cycle_converged = false;
        *self.verified_final.get_mut() = true;
    }

    /// Returns the semantic origin of this query.
    #[inline]
    pub(crate) const fn origin(&self) -> QueryOriginRef<'_> {
        self.origin_and_extra.origin()
    }

    #[inline]
    pub(crate) const fn is_derived_untracked(&self) -> bool {
        self.origin_and_extra.is_derived_untracked()
    }

    /// Discard dependency and output edges that a never-changing query will
    /// never need again.
    ///
    /// This is called after output reconciliation so that a query that becomes
    /// never-changing during re-execution can still preserve outputs recreated
    /// by that execution.
    #[cfg(not(feature = "persistence"))]
    pub(crate) fn discard_edges_if_never_change(&mut self) {
        if self.durability != Durability::NEVER_CHANGE
            || !matches!(self.origin(), QueryOriginRef::Derived(_))
            || !self.cycle_heads().is_empty()
        {
            return;
        }

        #[cfg(feature = "accumulator")]
        if self.accumulated_inputs.load().is_any() {
            return;
        }

        self.origin_and_extra.clear_edges();
    }

    #[cfg(feature = "salsa_unstable")]
    pub(crate) fn allocation_size(&self) -> usize {
        let QueryRevisions {
            changed_at: _,
            durability: _,
            verified_final: _,
            origin_and_extra,
            #[cfg(feature = "accumulator")]
                accumulated_inputs: _,
        } = self;

        origin_and_extra.allocation_size()
    }
}

pub(crate) struct PreparedIterationUpdate<'a> {
    iteration: PreparedIterationStore<'a>,
    own_head: Option<PreparedIterationStore<'a>>,
}

impl PreparedIterationUpdate<'_> {
    pub(crate) fn is_current(&self) -> bool {
        self.iteration.is_current()
            && self
                .own_head
                .as_ref()
                .is_none_or(PreparedIterationStore::is_current)
    }

    pub(crate) fn writes(&self) -> usize {
        1 + usize::from(self.own_head.is_some())
    }

    pub(crate) fn publish(self) {
        self.iteration.publish();
        if let Some(own_head) = self.own_head {
            own_head.publish();
        }
    }
}

/// Whether stored output positions follow the inputs that justified their creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum OutputOrder {
    Execution,
    /// Outputs copied from earlier executions retain ownership without their creation prerequisites.
    Inherited,
}

impl OutputOrder {
    #[cfg(feature = "persistence")]
    fn legacy() -> Self {
        // Older serialized metadata does not establish causal ordering for output edges.
        Self::Inherited
    }
}

/// Builder and serialization form for optional [`QueryRevisions`] data.
///
/// In particular, not all queries create tracked structs, participate
/// in cycles, or create accumulators. Stored revisions move this state
/// into the allocation owned by [`OriginAndExtra`].
#[derive(Debug, Default)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "persistence", serde(transparent))]
pub(crate) struct QueryRevisionsExtra(Option<QueryRevisionsExtraInner>);

impl QueryRevisionsExtra {
    pub fn new(
        #[cfg(feature = "accumulator")] accumulated: AccumulatedMap,
        mut tracked_struct_ids: ThinVec<(Identity, Id)>,
        cycle_heads: CycleHeads,
        iteration: IterationStamp,
        force_extra: bool,
        attempt_support: Option<AttemptSupport>,
        output_order: OutputOrder,
    ) -> Self {
        #[cfg(feature = "accumulator")]
        let accumulated_is_empty = accumulated.is_empty();
        #[cfg(not(feature = "accumulator"))]
        let accumulated_is_empty = true;
        // `cycle_heads` may be empty because cycle completion moved its entries out before
        // constructing the revisions. Preserve extra storage so those heads can be restored
        // without rebuilding the origin allocation.
        let inner = if !force_extra
            && accumulated_is_empty
            && tracked_struct_ids.is_empty()
            && cycle_heads.is_empty()
            && iteration.is_default()
            && attempt_support.is_none()
            && output_order == OutputOrder::Execution
        {
            None
        } else {
            tracked_struct_ids.shrink_to_fit();

            Some(QueryRevisionsExtraInner {
                #[cfg(feature = "accumulator")]
                accumulated,
                cycle_heads,
                tracked_struct_ids,
                iteration: iteration.into(),
                cycle_converged: false,
                attempt_support,
                execution_revision: None,
                output_order,
            })
        };

        Self(inner)
    }
}

#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
struct QueryRevisionsExtraInner {
    #[cfg_attr(feature = "persistence", serde(default = "OutputOrder::legacy"))]
    output_order: OutputOrder,

    #[cfg_attr(feature = "persistence", serde(skip))]
    attempt_support: Option<AttemptSupport>,

    /// The execution that produced this metadata, independent of later verification and
    /// backdating. Together with a head's accepted iteration and lineage, this identifies
    /// the component that may lazily finalize its participants. Restored final memos do
    /// not carry evidence about an in-memory cycle execution.
    #[cfg_attr(feature = "persistence", serde(skip))]
    execution_revision: Option<Revision>,
    #[cfg(feature = "accumulator")]
    #[cfg_attr(feature = "persistence", serde(skip))] // TODO: Support serializing accumulators
    accumulated: AccumulatedMap,

    /// The ids of tracked structs created by this query.
    ///
    /// This table plays an important role when queries are
    /// re-executed:
    /// * A clone of this field is used as the initial set of
    ///   `TrackedStructId`s for the query on the next execution.
    /// * The query will thus re-use the same ids if it creates
    ///   tracked structs with the same `KeyStruct` as before.
    ///   It may also create new tracked structs.
    /// * One tricky case involves deleted structs. If
    ///   the old revision created a struct S but the new
    ///   revision did not, there will still be a map entry
    ///   for S. This is because queries only ever grow the map
    ///   and they start with the same entries as from the
    ///   previous revision. To handle this, `diff_outputs` compares
    ///   the structs from the old/new revision and retains
    ///   only entries that appeared in the new revision.
    //
    // TODO: We only need to serialize the IDs of tracked structs that
    // are actually going to be serialized. Those that are not will
    // be created with new IDs anyways.
    tracked_struct_ids: ThinVec<(Identity, Id)>,

    /// This result was computed based on provisional values from
    /// these cycle heads. The "cycle head" is the query responsible
    /// for managing a fixpoint iteration. In a cycle like
    /// `--> A --> B --> C --> A`, the cycle head is query `A`: it is
    /// the query whose value is requested while it is executing,
    /// which must provide the initial provisional value and decide,
    /// after each iteration, whether the cycle has converged or must
    /// iterate again.
    cycle_heads: CycleHeads,

    #[cfg_attr(feature = "persistence", serde(skip))]
    iteration: AtomicIterationStamp,

    /// Stores for nested cycle heads whether they've converged in the last iteration.
    /// This value is always `false` for other queries.
    #[cfg_attr(feature = "persistence", serde(skip))]
    cycle_converged: bool,
}

impl QueryRevisionsExtraInner {
    fn empty() -> Self {
        QueryRevisionsExtraInner {
            output_order: OutputOrder::Execution,
            attempt_support: None,
            execution_revision: None,
            #[cfg(feature = "accumulator")]
            accumulated: AccumulatedMap::default(),
            tracked_struct_ids: ThinVec::default(),
            cycle_heads: empty_cycle_heads().clone(),
            iteration: IterationStamp::default().into(),
            cycle_converged: false,
        }
    }

    #[cfg(feature = "salsa_unstable")]
    fn allocation_size(&self) -> usize {
        let QueryRevisionsExtraInner {
            output_order: _,
            attempt_support: _,
            execution_revision: _,
            #[cfg(feature = "accumulator")]
            accumulated,
            tracked_struct_ids,
            cycle_heads,
            iteration: _,
            cycle_converged: _,
        } = self;

        #[cfg(feature = "accumulator")]
        let b = accumulated.allocation_size();
        #[cfg(not(feature = "accumulator"))]
        let b = 0;
        b + cycle_heads.allocation_size() + std::mem::size_of_val(tracked_struct_ids.as_slice())
    }
}

impl fmt::Debug for QueryRevisionsExtraInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        struct FmtTrackedStructIds<'a>(&'a ThinVec<(Identity, Id)>);

        impl fmt::Debug for FmtTrackedStructIds<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                let mut f = f.debug_list();

                if self.0.len() > 5 {
                    f.entries(&self.0[..5]);
                    f.finish_non_exhaustive()
                } else {
                    f.entries(self.0);
                    f.finish()
                }
            }
        }

        let mut f = f.debug_struct("QueryRevisionsExtraInner");

        f.field("cycle_heads", &self.cycle_heads)
            .field("output_order", &self.output_order)
            .field("attempt_support", &self.attempt_support)
            .field("execution_revision", &self.execution_revision)
            .field("iteration", &self.iteration)
            .field("cycle_converged", &self.cycle_converged);

        #[cfg(feature = "accumulator")]
        {
            f.field("accumulated", &self.accumulated);
        }

        f.field(
            "tracked_struct_ids",
            &FmtTrackedStructIds(&self.tracked_struct_ids),
        );

        f.finish()
    }
}

#[cfg(not(feature = "shuttle"))]
#[cfg(target_pointer_width = "64")]
const _: [(); std::mem::size_of::<QueryRevisions>()] = [(); std::mem::size_of::<[usize; 3]>()];

#[cfg(not(feature = "shuttle"))]
#[cfg(target_pointer_width = "64")]
const _: [(); std::mem::size_of::<QueryRevisionsExtraInner>()] =
    [(); std::mem::size_of::<[usize; if cfg!(feature = "accumulator") { 10 } else { 6 }]>()];

impl QueryRevisions {
    pub(crate) fn fixpoint_initial(
        zalsa: &Zalsa,
        query: DatabaseKeyIndex,
        iteration: IterationStamp,
    ) -> Self {
        Self {
            changed_at: Revision::start(),
            durability: Durability::MAX,
            origin_and_extra: OriginAndExtra::derived(
                std::iter::empty(),
                QueryRevisionsExtra::new(
                    #[cfg(feature = "accumulator")]
                    AccumulatedMap::default(),
                    ThinVec::default(),
                    CycleHeads::initial(query, iteration),
                    iteration,
                    false,
                    crate::attempt_probe::current_cycle_support(zalsa),
                    OutputOrder::Execution,
                ),
            ),
            #[cfg(feature = "accumulator")]
            accumulated_inputs: Default::default(),
            verified_final: AtomicBool::new(false),
        }
    }

    /// Returns a reference to the `AccumulatedMap` for this query, or `None` if the map is empty.
    #[cfg(feature = "accumulator")]
    pub(crate) fn accumulated(&self) -> Option<&AccumulatedMap> {
        self.origin_and_extra
            .extra()
            .map(|extra| &extra.accumulated)
            .filter(|map| !map.is_empty())
    }

    /// Returns a reference to the `CycleHeads` for this query.
    pub(crate) fn cycle_heads(&self) -> &CycleHeads {
        match self.origin_and_extra.extra() {
            Some(extra) => &extra.cycle_heads,
            None => empty_cycle_heads(),
        }
    }

    /// Sets the `CycleHeads` for this query.
    pub(crate) fn set_cycle_heads(&mut self, cycle_heads: CycleHeads, iteration: IterationStamp) {
        let extra = self.origin_and_extra.get_or_insert_extra();
        extra.cycle_heads = cycle_heads;
        extra.iteration = iteration.into();
    }

    pub(crate) fn set_provisional_support(&mut self, support: Option<AttemptSupport>) {
        assert!(!self.cycle_heads().is_empty());
        if support.is_some() {
            self.origin_and_extra.get_or_insert_extra().attempt_support = support;
        }
    }

    pub(crate) const fn cycle_converged(&self) -> bool {
        match self.origin_and_extra.extra() {
            Some(extra) => extra.cycle_converged,
            None => false,
        }
    }

    pub(crate) const fn set_cycle_converged(&mut self, cycle_converged: bool) {
        if let Some(extra) = self.origin_and_extra.extra_mut() {
            extra.cycle_converged = cycle_converged
        }
    }

    pub(crate) fn iteration(&self) -> IterationStamp {
        match self.origin_and_extra.extra() {
            Some(extra) => extra.iteration.load(),
            None => IterationStamp::default(),
        }
    }

    const fn extra(&self) -> Option<&QueryRevisionsExtraInner> {
        self.origin_and_extra.extra()
    }

    pub(crate) fn output_order(&self) -> OutputOrder {
        self.extra()
            .map_or(OutputOrder::Execution, |extra| extra.output_order)
    }

    /// Returns the ids of the tracked structs created when running this query.
    pub fn tracked_struct_ids(&self) -> &[(Identity, Id)] {
        self.extra()
            .map(|extra| &*extra.tracked_struct_ids)
            .unwrap_or_default()
    }

    /// Returns a mutable reference to the `IdentityMap` for this query, or `None` if the map is empty.
    pub fn tracked_struct_ids_mut(&mut self) -> Option<&mut ThinVec<(Identity, Id)>> {
        self.origin_and_extra
            .extra_mut()
            .map(|extra| &mut extra.tracked_struct_ids)
            .filter(|tracked_struct_ids| !tracked_struct_ids.is_empty())
    }
}

/// Tracks the way that a memoized value for a query was created.
///
/// This is a borrowed view of a query origin stored in [`OriginAndExtra`].
#[derive(Debug, Clone, Copy)]
pub enum QueryOriginRef<'a> {
    /// The value was assigned as the output of another query (e.g., using `specify`).
    /// The `DatabaseKeyIndex` is the identity of the assigning query.
    Assigned(DatabaseKeyIndex),

    /// The value was derived by executing a function
    /// and we were able to track ALL of that function's inputs.
    /// Those inputs are described in [`QueryEdges`].
    Derived(QueryEdges<'a>),

    /// The value was derived by executing a function
    /// but that function also reported that it read untracked inputs.
    /// The [`QueryEdges`] argument contains a listing of all the inputs we saw
    /// (but we know there were more).
    DerivedUntracked(QueryEdges<'a>),
}

impl<'a> QueryOriginRef<'a> {
    /// Indices for queries *read* by this query
    #[inline]
    pub(crate) fn inputs(self) -> impl DoubleEndedIterator<Item = DatabaseKeyIndex> + use<'a> {
        self.edges().iter().filter_map(|edge| match edge.kind() {
            QueryEdgeKind::Input => Some(edge.key()),
            QueryEdgeKind::Output => None,
        })
    }

    /// Indices for queries *written* by this query (if any)
    pub(crate) fn outputs(self) -> impl DoubleEndedIterator<Item = DatabaseKeyIndex> + use<'a> {
        let opt_edges = match self {
            QueryOriginRef::Derived(edges) | QueryOriginRef::DerivedUntracked(edges) => Some(edges),
            QueryOriginRef::Assigned(_) => None,
        };
        opt_edges.into_iter().flat_map(output_edges)
    }

    /// Bounds edge visits needed to find the first output; packed origins have no output edges.
    pub(crate) const fn output_scan_work(self) -> usize {
        match self.edges().data {
            QueryEdgesData::Packed(_) => 0,
            QueryEdgesData::Wide(edges) => edges.len(),
        }
    }

    #[inline]
    pub(crate) const fn edges(self) -> QueryEdges<'a> {
        match self {
            QueryOriginRef::Derived(edges) | QueryOriginRef::DerivedUntracked(edges) => edges,
            QueryOriginRef::Assigned(_) => QueryEdges::wide(&[]),
        }
    }
}

// The discriminants reserve bit 1 for derived origins, allowing `Derived` and
// `DerivedUntracked` to be recognized together.
#[derive(Clone, Copy)]
#[repr(u8)]
enum QueryOriginKind {
    /// The value was assigned as the output of another query.
    ///
    /// This can, for example, can occur when `specify` is used.
    Assigned = 0b01,

    /// The value was derived by executing a function
    /// _and_ Salsa was able to track all of said function's inputs.
    Derived = 0b11,

    /// The value was derived by executing a function
    /// but that function also reported that it read untracked inputs.
    DerivedUntracked = 0b10,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum DerivedOriginKind {
    Derived = QueryOriginKind::Derived as u8,
    DerivedUntracked = QueryOriginKind::DerivedUntracked as u8,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum QueryEdgeLayout {
    /// Every edge in the origin fits in [`PackedQueryEdge`].
    ///
    /// The origin stores one inline slice of `PackedQueryEdge`, reducing each retained edge from
    /// 12 bytes to 8 bytes.
    Packed = 0b000,

    /// At least one edge in the origin does not fit in [`PackedQueryEdge`].
    ///
    /// The origin stores one inline slice of `QueryEdge`. Spilling the entire origin avoids
    /// allocating a separate overflow object for each wide edge.
    Wide = 0b100,
}

/// Encodes the semantic origin kind and retained edge layout in a single byte.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct QueryOriginTag(u8);

impl QueryOriginTag {
    const KIND_MASK: u8 = 0b011;
    const LAYOUT_MASK: u8 = 0b100;

    const fn assigned() -> Self {
        QueryOriginTag(QueryOriginKind::Assigned as u8)
    }

    const fn derived(kind: DerivedOriginKind, layout: QueryEdgeLayout) -> Self {
        QueryOriginTag(kind as u8 | layout as u8)
    }

    const fn kind(self) -> QueryOriginKind {
        match self.0 & Self::KIND_MASK {
            0b01 => QueryOriginKind::Assigned,
            0b11 => QueryOriginKind::Derived,
            0b10 => QueryOriginKind::DerivedUntracked,
            _ => panic!("invalid query origin kind"),
        }
    }

    const fn layout(self) -> QueryEdgeLayout {
        if self.0 & Self::LAYOUT_MASK == 0 {
            QueryEdgeLayout::Packed
        } else {
            QueryEdgeLayout::Wide
        }
    }
}

/// Stores a query origin and optional revision data in one packed value.
///
/// The tag determines how `payload` and `metadata` must be interpreted:
///
/// | Origin   | Extra | `payload`                                       | `metadata`          |
/// |----------|-------|-------------------------------------------------|---------------------|
/// | Assigned | No    | Assigning query's [`Id`]                        | [`IngredientIndex`] |
/// | Assigned | Yes   | `Box<AssignedOriginAndExtra>`                   | [`IngredientIndex`] |
/// | Derived  | No    | `SliceWithHeader<(), E>`                       | Edge count          |
/// | Derived  | Yes   | `SliceWithHeader<QueryRevisionsExtraInner, E>` | Edge count          |
///
/// `E` is [`PackedQueryEdge`] or [`QueryEdge`], as recorded by [`QueryOriginTag`]. Assigned origins
/// without extra data require no allocation. Assigned origins with extra data use a regular box.
/// Derived origins always allocate their edge slice, placing the extra data in the same allocation
/// as the edges when it exists.
///
/// These combinations are invariants of the type: all accesses to the payload, allocation header,
/// and edge slice rely on the tag accurately describing the initialized representation.
#[repr(Rust, packed)]
pub(crate) struct OriginAndExtra {
    tag: OriginAndExtraTag,
    payload: OriginAndExtraPayload,
    metadata: u32,
}

const _: [(); std::mem::size_of::<OriginAndExtra>()] = [(); 13];

/// SAFETY: [`OriginAndExtra`] uses its tag and metadata to maintain the active payload field and its
/// allocation layout. Assigned origins contain an `Id`, directly or after an extra header. Derived
/// origins own an allocation containing packed or wide edges, optionally after an extra header.
unsafe impl Send for OriginAndExtra
where
    Id: Send,
    QueryRevisionsExtraInner: Send,
    PackedQueryEdge: Send,
    QueryEdge: Send,
{
}

/// SAFETY: Same as above, and shared access to every active value or allocation is `Sync`.
unsafe impl Sync for OriginAndExtra
where
    Id: Sync,
    QueryRevisionsExtraInner: Sync,
    PackedQueryEdge: Sync,
    QueryEdge: Sync,
{
}

impl OriginAndExtra {
    #[inline]
    pub(crate) fn derived<I>(input_outputs: I, extra: QueryRevisionsExtra) -> Self
    where
        I: ExactSizeIterator<Item = QueryEdge>,
    {
        Self::new_derived_with_kind(input_outputs, DerivedOriginKind::Derived, extra)
    }

    #[inline]
    pub(crate) fn derived_untracked<I>(input_outputs: I, extra: QueryRevisionsExtra) -> Self
    where
        I: ExactSizeIterator<Item = QueryEdge>,
    {
        Self::new_derived_with_kind(input_outputs, DerivedOriginKind::DerivedUntracked, extra)
    }

    pub(crate) const fn assigned(key: DatabaseKeyIndex) -> Self {
        Self {
            tag: OriginAndExtraTag::without_extra(QueryOriginTag::assigned()),
            payload: OriginAndExtraPayload {
                index: key.key_index(),
            },
            metadata: key.ingredient_index().as_u32(),
        }
    }

    fn assigned_with_extra(key: DatabaseKeyIndex, extra: QueryRevisionsExtraInner) -> Self {
        let allocation = Box::into_raw(Box::new(AssignedOriginAndExtra {
            extra,
            index: key.key_index(),
        }));
        Self {
            tag: OriginAndExtraTag::with_extra(QueryOriginTag::assigned()),
            payload: OriginAndExtraPayload {
                allocation: NonNull::new(allocation).unwrap().cast(),
            },
            metadata: key.ingredient_index().as_u32(),
        }
    }

    #[inline]
    fn new_derived_with_kind<I>(
        input_outputs: I,
        kind: DerivedOriginKind,
        extra: QueryRevisionsExtra,
    ) -> Self
    where
        I: ExactSizeIterator<Item = QueryEdge>,
    {
        match extra.0 {
            Some(extra) => Self::new_derived_with_extra(input_outputs, kind, extra),
            None => Self::new_derived_without_extra(input_outputs, kind),
        }
    }

    #[inline]
    fn new_derived_without_extra(
        input_outputs: impl ExactSizeIterator<Item = QueryEdge>,
        kind: DerivedOriginKind,
    ) -> Self {
        // SAFETY: The returned allocation uses `()` as its header. The tag records that there is no
        // extra header and records the returned edge layout.
        let (edge_layout, allocation, metadata) =
            unsafe { Self::allocate_derived_with_header(input_outputs, ()) };
        Self {
            tag: OriginAndExtraTag::without_extra(QueryOriginTag::derived(kind, edge_layout)),
            payload: OriginAndExtraPayload { allocation },
            metadata,
        }
    }

    #[inline]
    fn new_derived_with_extra(
        input_outputs: impl ExactSizeIterator<Item = QueryEdge>,
        kind: DerivedOriginKind,
        extra: QueryRevisionsExtraInner,
    ) -> Self {
        // SAFETY: The returned allocation uses `QueryRevisionsExtraInner` as its header. The tag
        // records that extra data is present and records the returned edge layout.
        let (edge_layout, allocation, metadata) =
            unsafe { Self::allocate_derived_with_header(input_outputs, extra) };
        Self {
            tag: OriginAndExtraTag::with_extra(QueryOriginTag::derived(kind, edge_layout)),
            payload: OriginAndExtraPayload { allocation },
            metadata,
        }
    }

    /// Allocates and initializes a derived origin with a header and inline edge slice.
    ///
    /// # Safety
    ///
    /// The returned pointer owns a completed `SliceWithHeader<H, E>` allocation, where `E` is
    /// selected by the returned edge layout. The caller must immediately store it in an
    /// [`OriginAndExtra`] whose tag records both whether `H` is `()` or
    /// `QueryRevisionsExtraInner` and the returned edge layout, and whose metadata is the returned
    /// length.
    #[inline]
    unsafe fn allocate_derived_with_header<H, I>(
        mut input_outputs: I,
        header: H,
    ) -> (QueryEdgeLayout, NonNull<()>, u32)
    where
        I: ExactSizeIterator<Item = QueryEdge>,
    {
        let length = input_outputs.len();
        let metadata = u32::try_from(length)
            .expect("exceeded more than `u32::MAX` query edges; this should never happen");
        let mut packed = SliceWithHeader::allocate(length);
        for edge in input_outputs.by_ref() {
            let Some(edge) = PackedQueryEdge::new(edge) else {
                let mut wide = SliceWithHeader::allocate(length);
                wide.extend(
                    packed
                        .initialized_slice()
                        .iter()
                        .copied()
                        .map(PackedQueryEdge::edge),
                );
                drop(packed);
                wide.push(edge);
                wide.extend(input_outputs);
                let allocation = wide.finish(header).into_raw();
                return (QueryEdgeLayout::Wide, allocation, metadata);
            };

            packed.push(edge);
        }

        let allocation = packed.finish(header).into_raw();
        (QueryEdgeLayout::Packed, allocation, metadata)
    }

    const fn extra(&self) -> Option<&QueryRevisionsExtraInner> {
        match self.tag.layout() {
            OriginAndExtraLayout::WithExtra => {
                // Every allocation with extra data puts that data first.
                // SAFETY: `payload.allocation` points to the live allocation owned by `self`.
                Some(unsafe {
                    self.payload
                        .allocation
                        .cast::<QueryRevisionsExtraInner>()
                        .as_ref()
                })
            }
            OriginAndExtraLayout::WithoutExtra => None,
        }
    }

    const fn extra_mut(&mut self) -> Option<&mut QueryRevisionsExtraInner> {
        match self.tag.layout() {
            OriginAndExtraLayout::WithExtra => {
                // Every allocation with extra data puts that data first.
                // SAFETY: The allocation is uniquely borrowed through `self`.
                Some(unsafe {
                    self.payload
                        .allocation
                        .cast::<QueryRevisionsExtraInner>()
                        .as_mut()
                })
            }
            OriginAndExtraLayout::WithoutExtra => None,
        }
    }

    fn get_or_insert_extra(&mut self) -> &mut QueryRevisionsExtraInner {
        if matches!(self.tag.layout(), OriginAndExtraLayout::WithoutExtra) {
            let extra = QueryRevisionsExtraInner::empty();
            let replacement = match self.origin() {
                QueryOriginRef::Assigned(key) => Self::assigned_with_extra(key, extra),
                QueryOriginRef::Derived(edges) => {
                    Self::derived(edges.iter(), QueryRevisionsExtra(Some(extra)))
                }
                QueryOriginRef::DerivedUntracked(edges) => {
                    Self::derived_untracked(edges.iter(), QueryRevisionsExtra(Some(extra)))
                }
            };
            *self = replacement;
        }

        self.extra_mut().unwrap()
    }

    #[cfg(not(feature = "persistence"))]
    fn clear_edges(&mut self) {
        // Avoid rebuilding the allocation when there are no edges to remove.
        if self.metadata == 0 {
            return;
        }

        let kind = match self.tag.origin().kind() {
            QueryOriginKind::Assigned => panic!("assigned query origins have no edges"),
            QueryOriginKind::Derived => DerivedOriginKind::Derived,
            QueryOriginKind::DerivedUntracked => DerivedOriginKind::DerivedUntracked,
        };

        let extra = self
            .extra_mut()
            .map(|extra| std::mem::replace(extra, QueryRevisionsExtraInner::empty()));
        *self = Self::new_derived_with_kind([].into_iter(), kind, QueryRevisionsExtra(extra));
    }

    const fn is_derived_untracked(&self) -> bool {
        matches!(self.tag.origin().kind(), QueryOriginKind::DerivedUntracked)
    }

    const fn origin(&self) -> QueryOriginRef<'_> {
        let tag = self.tag.origin();
        match tag.kind() {
            QueryOriginKind::Assigned => {
                let index = match self.tag.layout() {
                    OriginAndExtraLayout::WithoutExtra => {
                        // SAFETY: Direct assigned origins initialize `payload.index`.
                        unsafe { self.payload.index }
                    }
                    OriginAndExtraLayout::WithExtra => {
                        // SAFETY: Indirect assigned origins use `AssignedOriginAndExtra`.
                        unsafe {
                            self.payload
                                .allocation
                                .cast::<AssignedOriginAndExtra>()
                                .as_ref()
                                .index
                        }
                    }
                };
                // SAFETY: Assigned origins initialize metadata from a valid ingredient index.
                let ingredient = unsafe { IngredientIndex::new_unchecked(self.metadata) };
                QueryOriginRef::Assigned(DatabaseKeyIndex::new(ingredient, index))
            }
            QueryOriginKind::Derived | QueryOriginKind::DerivedUntracked => {
                let length = self.metadata as usize;
                // SAFETY: Derived origins initialize `payload.allocation`, and the tag records the
                // header and edge layout used to create that allocation.
                let edges = unsafe {
                    let allocation = self.payload.allocation;
                    match (self.tag.layout(), tag.layout()) {
                        (OriginAndExtraLayout::WithoutExtra, QueryEdgeLayout::Packed) => {
                            QueryEdges::packed(
                                SliceWithHeader::<(), PackedQueryEdge>::slice(allocation, length)
                                    .as_ref(),
                            )
                        }
                        (OriginAndExtraLayout::WithoutExtra, QueryEdgeLayout::Wide) => {
                            QueryEdges::wide(
                                SliceWithHeader::<(), QueryEdge>::slice(allocation, length).as_ref(),
                            )
                        }
                        (OriginAndExtraLayout::WithExtra, QueryEdgeLayout::Packed) => {
                            QueryEdges::packed(
                                SliceWithHeader::<QueryRevisionsExtraInner, PackedQueryEdge>::slice(
                                    allocation, length,
                                )
                                .as_ref(),
                            )
                        }
                        (OriginAndExtraLayout::WithExtra, QueryEdgeLayout::Wide) => {
                            QueryEdges::wide(
                                SliceWithHeader::<QueryRevisionsExtraInner, QueryEdge>::slice(
                                    allocation, length,
                                )
                                .as_ref(),
                            )
                        }
                    }
                };
                match tag.kind() {
                    QueryOriginKind::Derived => QueryOriginRef::Derived(edges),
                    QueryOriginKind::DerivedUntracked => QueryOriginRef::DerivedUntracked(edges),
                    QueryOriginKind::Assigned => unreachable!(),
                }
            }
        }
    }

    #[cfg(any(test, feature = "salsa_unstable"))]
    fn allocation_size(&self) -> usize {
        let tag = self.tag.origin();
        let memory = match (self.tag.layout(), tag.kind(), tag.layout()) {
            (OriginAndExtraLayout::WithoutExtra, QueryOriginKind::Assigned, _) => 0,
            (OriginAndExtraLayout::WithExtra, QueryOriginKind::Assigned, _) => {
                std::mem::size_of::<AssignedOriginAndExtra>()
            }
            (_, _, QueryEdgeLayout::Packed) => self.derived_allocation_size::<PackedQueryEdge>(),
            (_, _, QueryEdgeLayout::Wide) => self.derived_allocation_size::<QueryEdge>(),
        };

        #[cfg(feature = "salsa_unstable")]
        if let Some(extra) = self.extra() {
            return memory + extra.allocation_size();
        }

        memory
    }

    #[cfg(any(test, feature = "salsa_unstable"))]
    const fn derived_allocation_size<E: Copy>(&self) -> usize {
        let length = self.metadata as usize;
        match self.tag.layout() {
            OriginAndExtraLayout::WithoutExtra => SliceWithHeader::<(), E>::layout(length).0.size(),
            OriginAndExtraLayout::WithExtra => {
                SliceWithHeader::<QueryRevisionsExtraInner, E>::layout(length)
                    .0
                    .size()
            }
        }
    }
}

impl Drop for OriginAndExtra {
    fn drop(&mut self) {
        let tag = self.tag.origin();
        match (tag.kind(), self.tag.layout(), tag.layout()) {
            (QueryOriginKind::Assigned, OriginAndExtraLayout::WithoutExtra, _) => {}
            (QueryOriginKind::Assigned, OriginAndExtraLayout::WithExtra, _) => {
                // SAFETY: Indirect assigned origins use `Box<AssignedOriginAndExtra>`.
                let allocation = unsafe { self.payload.allocation };
                // SAFETY: The allocation was created by `Box::into_raw` for this type.
                drop(unsafe {
                    Box::from_raw(allocation.cast::<AssignedOriginAndExtra>().as_ptr())
                });
            }
            (QueryOriginKind::Derived | QueryOriginKind::DerivedUntracked, layout, edge_layout) => {
                let length = self.metadata as usize;
                // SAFETY: Derived origins initialize `payload.allocation`, and the tag records the
                // header and edge layout used to create that allocation.
                unsafe {
                    let allocation = self.payload.allocation;
                    match (layout, edge_layout) {
                        (OriginAndExtraLayout::WithoutExtra, QueryEdgeLayout::Packed) => drop(
                            SliceWithHeader::<(), PackedQueryEdge>::from_raw_parts(
                                allocation, length,
                            ),
                        ),
                        (OriginAndExtraLayout::WithoutExtra, QueryEdgeLayout::Wide) => drop(
                            SliceWithHeader::<(), QueryEdge>::from_raw_parts(allocation, length),
                        ),
                        (OriginAndExtraLayout::WithExtra, QueryEdgeLayout::Packed) => drop(
                            SliceWithHeader::<QueryRevisionsExtraInner, PackedQueryEdge>::from_raw_parts(
                                allocation, length,
                            ),
                        ),
                        (OriginAndExtraLayout::WithExtra, QueryEdgeLayout::Wide) => drop(
                            SliceWithHeader::<QueryRevisionsExtraInner, QueryEdge>::from_raw_parts(
                                allocation, length,
                            ),
                        ),
                    }
                }
            }
        }
    }
}

impl std::fmt::Debug for OriginAndExtra {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(extra) = self.extra() {
            f.debug_struct("OriginAndExtra")
                .field("origin", &self.origin())
                .field("extra", extra)
                .finish()
        } else {
            self.origin().fmt(f)
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum OriginAndExtraLayout {
    /// The origin has no extra revision data.
    WithoutExtra = 0b0000,

    /// The origin's allocation begins with extra revision data.
    WithExtra = 0b1000,
}

/// Encodes whether the origin has extra revision data.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct OriginAndExtraTag(u8);

impl OriginAndExtraTag {
    const WITH_EXTRA_MASK: u8 = OriginAndExtraLayout::WithExtra as u8;

    const fn without_extra(origin: QueryOriginTag) -> Self {
        OriginAndExtraTag(origin.0)
    }

    const fn with_extra(origin: QueryOriginTag) -> Self {
        OriginAndExtraTag(OriginAndExtraLayout::WithExtra as u8 | origin.0)
    }

    const fn layout(self) -> OriginAndExtraLayout {
        if self.0 & Self::WITH_EXTRA_MASK == 0 {
            OriginAndExtraLayout::WithoutExtra
        } else {
            OriginAndExtraLayout::WithExtra
        }
    }

    const fn origin(self) -> QueryOriginTag {
        QueryOriginTag(self.0 & !Self::WITH_EXTRA_MASK)
    }
}

/// The payload of [`OriginAndExtra`].
union OriginAndExtraPayload {
    /// The allocation storing derived edges and, for origins with extra data, that data.
    allocation: NonNull<()>,

    /// The identity of the assigning query for a directly stored assigned origin.
    index: Id,
}

#[repr(C)]
struct AssignedOriginAndExtra {
    extra: QueryRevisionsExtraInner,
    index: Id,
}

/// Owns one allocation containing a header followed by an inline slice:
///
/// ```text
/// +-----------+---------+---------+-----+
/// | header: H | item: T | item: T | ... |
/// +-----------+---------+---------+-----+
/// ```
///
/// This type defines the physical layout and owns completed allocations reconstructed from
/// [`OriginAndExtra`]. `OriginAndExtra` records the semantic origin and selects the concrete `H`
/// and `T` encoded by its tag. Derived origins use `()` or [`QueryRevisionsExtraInner`] as the
/// header and [`PackedQueryEdge`] or [`QueryEdge`] as the slice element.
struct SliceWithHeader<H, T: Copy> {
    allocation: NonNull<()>,
    layout: Layout,
    marker: PhantomData<(H, T)>,
}

impl<H, T: Copy> SliceWithHeader<H, T> {
    /// Allocates storage for the header and slice, returning a builder that initializes the slice
    /// incrementally. Construction may switch from packed to wide edges, so the caller retains the
    /// header until it finishes the selected builder. The builder tracks the initialized prefix and
    /// deallocates unfinished storage on drop. Once complete, the allocation is transferred to
    /// [`OriginAndExtra`], which reconstructs borrowed slices or owned [`SliceWithHeader`] values
    /// using the layout encoded in its tag.
    #[inline]
    fn allocate(length: usize) -> SliceWithHeaderBuilder<H, T> {
        let (layout, slice_offset) = Self::layout(length);
        let allocation = if layout.size() == 0 {
            if std::mem::align_of::<H>() >= std::mem::align_of::<T>() {
                NonNull::<H>::dangling().cast()
            } else {
                NonNull::<T>::dangling().cast()
            }
        } else {
            // SAFETY: `layout` has non-zero size.
            unsafe { NonNull::new(alloc(layout)) }.unwrap_or_else(|| handle_alloc_error(layout))
        }
        .cast();

        // SAFETY: `slice_offset` was computed from the layout used for this allocation.
        let slice = unsafe { allocation.cast::<u8>().byte_add(slice_offset).cast::<T>() };
        SliceWithHeaderBuilder {
            allocation,
            slice,
            layout,
            length,
            initialized: 0,
            marker: PhantomData,
        }
    }

    const fn layout(length: usize) -> (Layout, usize) {
        let slice = match Layout::array::<T>(length) {
            Ok(slice) => slice,
            Err(_) => panic!("slice allocation is too large"),
        };
        let (layout, slice_offset) = match Layout::new::<H>().extend(slice) {
            Ok(layout) => layout,
            Err(_) => panic!("slice-with-header allocation is too large"),
        };
        (layout, slice_offset)
    }

    /// # Safety
    ///
    /// `allocation` must point to a live allocation completed by
    /// `SliceWithHeaderBuilder::<H, T>::finish` for exactly `length` elements. Its header and all
    /// elements must remain initialized and the allocation must remain alive for the lifetime of
    /// the returned slice.
    const unsafe fn slice(allocation: NonNull<()>, length: usize) -> NonNull<[T]> {
        let (_, slice_offset) = Self::layout(length);
        // SAFETY: Caller obligation and the offset was computed from the matching layout.
        let slice = unsafe { allocation.cast::<u8>().byte_add(slice_offset).cast::<T>() };
        NonNull::slice_from_raw_parts(slice, length)
    }

    /// # Safety
    ///
    /// `allocation` must point to a live allocation completed by
    /// `SliceWithHeaderBuilder::<H, T>::finish` for exactly `length` elements. Its header and all
    /// elements must be initialized, and the caller must transfer unique ownership of the
    /// allocation to the returned value.
    const unsafe fn from_raw_parts(allocation: NonNull<()>, length: usize) -> Self {
        SliceWithHeader {
            allocation,
            layout: Self::layout(length).0,
            marker: PhantomData,
        }
    }

    fn into_raw(self) -> NonNull<()> {
        ManuallyDrop::new(self).allocation
    }
}

impl<H, T: Copy> Drop for SliceWithHeader<H, T> {
    fn drop(&mut self) {
        // SAFETY: The allocation starts with an initialized `H` and is uniquely owned. `T` is
        // `Copy`, so no slice elements require dropping.
        unsafe { ptr::drop_in_place(self.allocation.cast::<H>().as_ptr()) };
        if self.layout.size() != 0 {
            // SAFETY: `Drop` runs exactly once while the allocation is uniquely owned.
            unsafe { dealloc(self.allocation.cast().as_ptr(), self.layout) };
        }
    }
}

/// Builds a [`SliceWithHeader`] allocation while tracking its initialized prefix.
struct SliceWithHeaderBuilder<H, T: Copy> {
    allocation: NonNull<()>,
    slice: NonNull<T>,
    layout: Layout,
    length: usize,
    initialized: usize,
    marker: PhantomData<fn() -> H>,
}

impl<H, T: Copy> SliceWithHeaderBuilder<H, T> {
    #[inline]
    const fn push(&mut self, item: T) {
        assert!(
            self.initialized < self.length,
            "attempted to initialize more slice elements than allocated"
        );
        // SAFETY: The initialized prefix is shorter than the allocation and this element has not
        // been initialized yet.
        unsafe { self.slice.add(self.initialized).write(item) };
        self.initialized += 1;
    }

    #[inline]
    fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.push(item);
        }
    }

    #[inline]
    const fn initialized_slice(&self) -> &[T] {
        // SAFETY: `initialized` tracks the contiguous prefix written by `push`.
        unsafe { std::slice::from_raw_parts(self.slice.as_ptr(), self.initialized) }
    }

    #[inline]
    fn finish(self, header: H) -> SliceWithHeader<H, T> {
        assert_eq!(
            self.initialized, self.length,
            "not all allocated slice elements were initialized"
        );
        let this = ManuallyDrop::new(self);
        // SAFETY: The allocation starts with space for `H` and the header is initialized once.
        unsafe { this.allocation.cast().write(header) };
        SliceWithHeader {
            allocation: this.allocation,
            layout: this.layout,
            marker: PhantomData,
        }
    }
}

impl<H, T: Copy> Drop for SliceWithHeaderBuilder<H, T> {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: `allocation` was created with `layout` and is still uniquely owned.
            unsafe { dealloc(self.allocation.cast().as_ptr(), self.layout) };
        }
    }
}

/// An input or output query edge.
///
/// This type stores the [`QueryEdgeKind`] as a tag on the `IngredientIndex` without
/// increasing the size of the type. Its 12-byte size is meaningful because inputs and
/// outputs are stored contiguously.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct QueryEdge {
    // Store a normalized zero-based index rather than nesting an `Id`, whose index uses a
    // `NonZeroU32(index + 1)` representation. Packed origins unpack many transient
    // `QueryEdge`s while flattening cycles; keeping the fields split lets that conversion,
    // `kind`, `Hash`, and `Eq` operate on the decoded words directly.
    index: u32,
    generation: u32,
    ingredient: IngredientIndex,
}

const _: [(); std::mem::size_of::<QueryEdge>()] = [(); 12];

impl QueryEdge {
    /// Create an input query edge with the given index.
    pub const fn input(key: DatabaseKeyIndex) -> QueryEdge {
        let id = key.key_index();

        QueryEdge {
            index: id.index(),
            generation: id.generation(),
            ingredient: key.ingredient_index(),
        }
    }

    /// Create an output query edge with the given index.
    pub const fn output(key: DatabaseKeyIndex) -> QueryEdge {
        let id = key.key_index();

        QueryEdge {
            index: id.index(),
            generation: id.generation(),
            ingredient: key.ingredient_index().with_tag(true),
        }
    }

    /// Return the key of this query edge.
    pub const fn key(self) -> DatabaseKeyIndex {
        // Clear the tag to restore the original index.
        DatabaseKeyIndex::new(self.ingredient.with_tag(false), self.id())
    }

    /// Returns the kind of this query edge.
    pub const fn kind(self) -> QueryEdgeKind {
        if self.ingredient.tag() {
            QueryEdgeKind::Output
        } else {
            QueryEdgeKind::Input
        }
    }

    const fn id(self) -> Id {
        // SAFETY: `index` came from a valid `Id` in `QueryEdge::input`.
        unsafe { Id::from_index(self.index) }.with_generation(self.generation)
    }
}

impl std::hash::Hash for QueryEdge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let &Self {
            index,
            ingredient,
            generation: _,
        } = self;
        // A query should never depend on the same ingredient and index at different generations:
        // advancing the generation replaces the previous identity. If this assumption is ever
        // violated, `Eq` still compares the generation, so omitting it here only causes a hash
        // collision. The ingredient's tag bit includes the edge kind in the hash.
        state.write_u32(index);
        state.write_u32(ingredient.as_u32());
    }
}

impl std::fmt::Debug for QueryEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tuple = match self.kind() {
            QueryEdgeKind::Input => f.debug_tuple("Input"),
            QueryEdgeKind::Output => f.debug_tuple("Output"),
        };
        tuple.field(&self.key()).finish()
    }
}

/// A retained 8-byte query edge.
///
/// Query origins use this representation when every edge fits. Otherwise, the origin
/// retains the existing full-width [`QueryEdge`] slice.
///
/// `metadata` has the following layout, from most significant to least significant
/// bit:
///
/// ```text
/// [ingredient: 12][generation: 20]
/// ```
///
/// `index` stores the complete 32-bit key index separately. Only input edges can use
/// this representation. Output edges, ingredient indices, and generations that do
/// not fit use the wide [`QueryEdge`] layout.
#[derive(Copy, Clone)]
struct PackedQueryEdge {
    index: u32,
    metadata: u32,
}

const _: [(); std::mem::size_of::<PackedQueryEdge>()] = [(); 8];

impl PackedQueryEdge {
    const INGREDIENT_SHIFT: u32 = 20;
    const GENERATION_MASK: u32 = 0xFFFFF;
    const INGREDIENT_MASK: u32 = 0xFFF;

    const fn new(edge: QueryEdge) -> Option<Self> {
        let ingredient = edge.ingredient.as_u32();

        if ingredient > Self::INGREDIENT_MASK || edge.generation > Self::GENERATION_MASK {
            return None;
        }

        Some(PackedQueryEdge {
            index: edge.index,
            metadata: edge.generation | (ingredient << Self::INGREDIENT_SHIFT),
        })
    }

    const fn edge(self) -> QueryEdge {
        QueryEdge {
            index: self.index,
            generation: self.metadata & Self::GENERATION_MASK,
            // SAFETY: `metadata` was built from an `IngredientIndex` in `PackedQueryEdge::new`.
            ingredient: unsafe {
                IngredientIndex::new_unchecked(self.metadata >> Self::INGREDIENT_SHIFT)
            },
        }
    }
}

/// A read-only view over the retained edges for a query origin.
#[derive(Copy, Clone)]
pub struct QueryEdges<'a> {
    data: QueryEdgesData<'a>,
}

#[derive(Copy, Clone)]
enum QueryEdgesData<'a> {
    Packed(&'a [PackedQueryEdge]),
    Wide(&'a [QueryEdge]),
}

impl<'a> QueryEdges<'a> {
    const fn packed(edges: &'a [PackedQueryEdge]) -> Self {
        QueryEdges {
            data: QueryEdgesData::Packed(edges),
        }
    }

    const fn wide(edges: &'a [QueryEdge]) -> Self {
        QueryEdges {
            data: QueryEdgesData::Wide(edges),
        }
    }

    #[cfg(feature = "accumulator")]
    pub(crate) const fn len(self) -> usize {
        match self.data {
            QueryEdgesData::Packed(edges) => edges.len(),
            QueryEdgesData::Wide(edges) => edges.len(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn allocation_size(self) -> usize {
        match self.data {
            QueryEdgesData::Packed(edges) => std::mem::size_of_val(edges),
            QueryEdgesData::Wide(edges) => std::mem::size_of_val(edges),
        }
    }

    pub(crate) fn iter(self) -> QueryEdgeIter<'a> {
        let data = match self.data {
            QueryEdgesData::Packed(edges) => QueryEdgeIterData::Packed(edges.iter()),
            QueryEdgesData::Wide(edges) => QueryEdgeIterData::Wide(edges.iter()),
        };

        QueryEdgeIter { data }
    }

    pub(crate) fn iter_outputs(self) -> impl DoubleEndedIterator<Item = QueryEdge> + use<'a> {
        let wide_edges = match self.data {
            QueryEdgesData::Packed(_) => &[][..],
            QueryEdgesData::Wide(edges) => edges,
        };

        wide_edges
            .iter()
            .copied()
            .filter(|edge| matches!(edge.kind(), QueryEdgeKind::Output))
    }

    #[cfg(test)]
    const fn is_packed(self) -> bool {
        matches!(self.data, QueryEdgesData::Packed(_))
    }
}

pub struct QueryEdgeIter<'a> {
    data: QueryEdgeIterData<'a>,
}

enum QueryEdgeIterData<'a> {
    Packed(std::slice::Iter<'a, PackedQueryEdge>),
    Wide(std::slice::Iter<'a, QueryEdge>),
}

impl Iterator for QueryEdgeIter<'_> {
    type Item = QueryEdge;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.data {
            QueryEdgeIterData::Packed(edges) => edges.next().copied().map(PackedQueryEdge::edge),
            QueryEdgeIterData::Wide(edges) => edges.next().copied(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for QueryEdgeIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.data {
            QueryEdgeIterData::Packed(edges) => {
                edges.next_back().copied().map(PackedQueryEdge::edge)
            }
            QueryEdgeIterData::Wide(edges) => edges.next_back().copied(),
        }
    }
}

impl ExactSizeIterator for QueryEdgeIter<'_> {
    fn len(&self) -> usize {
        match &self.data {
            QueryEdgeIterData::Packed(edges) => edges.len(),
            QueryEdgeIterData::Wide(edges) => edges.len(),
        }
    }
}

impl std::iter::FusedIterator for QueryEdgeIter<'_> {}

impl<'a> IntoIterator for QueryEdges<'a> {
    type Item = QueryEdge;
    type IntoIter = QueryEdgeIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Debug for QueryEdges<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum QueryEdgeKind {
    Input,
    Output,
}

/// Returns the (tracked) outputs that were executed in computing this memoized value.
///
/// These will always be in execution order.
pub(crate) fn output_edges(
    input_outputs: QueryEdges<'_>,
) -> impl DoubleEndedIterator<Item = DatabaseKeyIndex> + use<'_> {
    input_outputs.iter_outputs().map(QueryEdge::key)
}

/// When a query is pushed onto the `active_query` stack, this guard
/// is returned to represent its slot. The guard can be used to pop
/// the query from the stack -- in the case of unwinding, the guard's
/// destructor will also remove the query.
pub(crate) struct ActiveQueryGuard<'me> {
    local_state: &'me ZalsaLocal,
    #[cfg(debug_assertions)]
    push_len: usize,
    pub(crate) database_key_index: DatabaseKeyIndex,
}

impl<'me> ActiveQueryGuard<'me> {
    pub(crate) fn is_current_at_depth(&self, depth: usize) -> bool {
        self.local_state.try_with_query_stack(|stack| {
            stack.len() == depth
                && stack
                    .last()
                    .is_some_and(|query| query.database_key_index == self.database_key_index)
        }) == Some(true)
    }

    pub(crate) fn abort_return_only(self, depth: usize) {
        assert!(
            self.is_current_at_depth(depth),
            "aborting an inactive query frame"
        );
        self.finish_attempt_incomplete();
        // Discard the unfinished frame without creating revisions or publishing a value.
        drop(self);
    }

    pub(crate) fn attempt_incomplete(&self) -> bool {
        // SAFETY: This callback does not access the query stack recursively.
        unsafe {
            self.local_state.with_query_stack_unchecked(|stack| {
                stack.last().is_some_and(|query| query.attempt_incomplete())
            })
        }
    }

    pub(crate) fn finish_attempt_incomplete(&self) {
        // SAFETY: This callback does not access the query stack recursively.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                stack
                    .last_mut()
                    .expect("active query missing")
                    .finish_attempt_incomplete();
            });
        }
    }

    /// Initialize the tracked struct ids with the values from the prior execution.
    pub(crate) fn seed_tracked_struct_ids(&self, tracked_struct_ids: &[(Identity, Id)]) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                #[cfg(debug_assertions)]
                assert_eq!(stack.len(), self.push_len, "mismatched push and pop");
                let frame = stack.last_mut().unwrap();
                frame.tracked_struct_ids_mut().seed(tracked_struct_ids);
            })
        }
    }

    /// Append the given `outputs` to the query's output list.
    pub(crate) fn seed_iteration(&self, zalsa: &Zalsa, previous: &QueryRevisions) {
        let durability = previous.durability;
        let changed_at = previous.changed_at;
        let edges = previous.origin().edges();
        let untracked_read = previous.is_derived_untracked();

        let tracked_ids = previous.tracked_struct_ids();

        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                #[cfg(debug_assertions)]
                assert_eq!(stack.len(), self.push_len, "mismatched push and pop");
                let frame = stack.last_mut().unwrap();
                if let Some(support) = previous.attempt_support() {
                    frame.add_attempt_support(zalsa, support, previous.has_incomplete_attempt());
                }
                frame.seed_iteration(durability, changed_at, edges, untracked_read, tracked_ids);
            })
        }
    }

    pub(crate) fn seed_output_ownership(&self, edges: QueryEdges<'_>, ids: &[(Identity, Id)]) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                #[cfg(debug_assertions)]
                assert_eq!(stack.len(), self.push_len, "mismatched push and pop");
                stack.last_mut().unwrap().seed_output_ownership(edges, ids);
            });
        }
    }

    pub(crate) fn take_cycle_heads(&mut self) -> CycleHeads {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                #[cfg(debug_assertions)]
                assert_eq!(stack.len(), self.push_len);
                let frame = stack.last_mut().unwrap();
                frame.take_cycle_heads()
            })
        }
    }

    pub(crate) fn detach(self) -> DetachedQuery<'me> {
        // SAFETY: We do not access the query stack reentrantly.
        let input_outputs = unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                #[cfg(debug_assertions)]
                assert_eq!(stack.len(), self.push_len);
                stack.last_mut().unwrap().detach_input_outputs()
            })
        };
        DetachedQuery {
            guard: self,
            input_outputs,
        }
    }

    /// Invoked when the query has successfully completed execution.
    fn complete(self, iteration: IterationStamp) -> CompletedQuery {
        // SAFETY: We do not access the query stack reentrantly.
        let query = unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                stack.pop_into_revisions(
                    self.database_key_index,
                    iteration,
                    #[cfg(debug_assertions)]
                    self.push_len,
                )
            })
        };
        std::mem::forget(self);
        query
    }

    /// Pops an active query from the stack. Returns the [`CompletedQuery`]
    /// which summarizes the other queries that were accessed during this
    /// query's execution.
    #[inline]
    pub(crate) fn pop(self, iteration: IterationStamp) -> CompletedQuery {
        self.complete(iteration)
    }

    fn pop_detached_completion(
        self,
        iteration: IterationStamp,
        detached_input_outputs: DetachedInputOutputs,
        force_extra: bool,
    ) -> QueryCompletion {
        // SAFETY: We do not access the query stack reentrantly.
        let completion = unsafe {
            self.local_state
                .with_query_stack_unchecked_mut(move |stack| {
                    stack.pop_detached_completion(
                        self.database_key_index,
                        iteration,
                        detached_input_outputs,
                        force_extra,
                        #[cfg(debug_assertions)]
                        self.push_len,
                    )
                })
        };
        std::mem::forget(self);
        completion
    }
}

pub(crate) struct DetachedQuery<'me> {
    guard: ActiveQueryGuard<'me>,
    input_outputs: DetachedInputOutputs,
}

impl DetachedQuery<'_> {
    pub(crate) fn input_outputs(&self) -> &crate::hash::FxIndexSet<QueryEdge> {
        self.input_outputs.input_outputs()
    }

    pub(crate) fn pop_completion(
        self,
        iteration: IterationStamp,
        force_extra: bool,
    ) -> QueryCompletion {
        let Self {
            guard,
            input_outputs,
        } = self;
        guard.pop_detached_completion(iteration, input_outputs, force_extra)
    }
}

impl Drop for ActiveQueryGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: We do not access the query stack reentrantly.
        unsafe {
            self.local_state.with_query_stack_unchecked_mut(|stack| {
                stack.pop(
                    self.database_key_index,
                    #[cfg(debug_assertions)]
                    self.push_len,
                );
            })
        };
    }
}

#[cfg(feature = "persistence")]
pub(crate) mod persistence {
    #[cfg(test)]
    use super::QueryOriginRef;
    use super::{OriginAndExtra, QueryEdge, QueryEdges, QueryRevisions, QueryRevisionsExtra};
    use crate::DatabaseKeyIndex;
    use crate::sync::atomic::{AtomicBool, Ordering};
    use crate::{Durability, Revision};

    impl OriginAndExtra {
        pub(crate) fn new(origin: PersistentQueryOrigin, extra: QueryRevisionsExtra) -> Self {
            match origin {
                PersistentQueryOrigin::Assigned(key) => match extra.0 {
                    Some(extra) => Self::assigned_with_extra(key, extra),
                    None => Self::assigned(key),
                },
                PersistentQueryOrigin::Derived(edges) => {
                    Self::derived(edges.iter().copied(), extra)
                }
                PersistentQueryOrigin::DerivedUntracked(edges) => {
                    Self::derived_untracked(edges.iter().copied(), extra)
                }
            }
        }
    }

    impl QueryEdge {
        const fn raw_key(self) -> DatabaseKeyIndex {
            DatabaseKeyIndex::new(self.ingredient, self.id())
        }
    }

    impl serde::Serialize for QueryEdge {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serde::Serialize::serialize(&self.raw_key(), serializer)
        }
    }

    impl<'de> serde::Deserialize<'de> for QueryEdge {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let key: DatabaseKeyIndex = serde::Deserialize::deserialize(deserializer)?;
            let id = key.key_index();

            Ok(QueryEdge {
                index: id.index(),
                generation: id.generation(),
                ingredient: key.ingredient_index(),
            })
        }
    }

    impl serde::Serialize for QueryEdges<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.collect_seq(self.iter())
        }
    }

    /// Safe owning representation of a query origin used for persistence.
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(rename = "QueryOrigin")]
    pub(crate) enum PersistentQueryOrigin {
        Assigned(DatabaseKeyIndex),
        Derived(Box<[QueryEdge]>),
        DerivedUntracked(Box<[QueryEdge]>),
    }

    impl PersistentQueryOrigin {
        pub(crate) fn derived<I>(input_outputs: I) -> Self
        where
            I: IntoIterator<Item = QueryEdge>,
            I::IntoIter: ExactSizeIterator,
        {
            Self::Derived(input_outputs.into_iter().collect())
        }

        pub(crate) fn derived_untracked<I>(input_outputs: I) -> Self
        where
            I: IntoIterator<Item = QueryEdge>,
            I::IntoIter: ExactSizeIterator,
        {
            Self::DerivedUntracked(input_outputs.into_iter().collect())
        }

        pub(crate) const fn assigned(key: DatabaseKeyIndex) -> Self {
            Self::Assigned(key)
        }

        #[cfg(test)]
        pub(crate) const fn as_ref(&self) -> QueryOriginRef<'_> {
            match self {
                Self::Assigned(key) => QueryOriginRef::Assigned(*key),
                Self::Derived(edges) => QueryOriginRef::Derived(QueryEdges::wide(edges)),
                Self::DerivedUntracked(edges) => {
                    QueryOriginRef::DerivedUntracked(QueryEdges::wide(edges))
                }
            }
        }
    }

    /// A serialization view that separates the packed origin from optional revision data.
    #[derive(serde::Serialize)]
    pub(crate) struct MappedQueryRevisions<'a> {
        changed_at: Revision,
        durability: Durability,
        #[serde(rename = "origin")]
        origin: PersistentQueryOrigin,
        #[serde(with = "verified_final")]
        verified_final: AtomicBool,
        extra: Option<&'a super::QueryRevisionsExtraInner>,
    }

    impl QueryRevisions {
        pub(crate) fn with_origin(
            &self,
            serialized_origin: PersistentQueryOrigin,
        ) -> MappedQueryRevisions<'_> {
            let QueryRevisions {
                changed_at,
                durability,
                ref verified_final,
                #[cfg(feature = "accumulator")]
                    accumulated_inputs: _, // TODO: Support serializing accumulators
                ref origin_and_extra,
            } = *self;

            MappedQueryRevisions {
                changed_at,
                durability,
                extra: origin_and_extra.extra(),
                origin: serialized_origin,
                verified_final: AtomicBool::new(verified_final.load(Ordering::Acquire)),
            }
        }
    }

    impl<'de> serde::Deserialize<'de> for QueryRevisions {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(serde::Deserialize)]
            struct DeserializeQueryRevisions {
                changed_at: Revision,
                durability: Durability,
                #[serde(rename = "origin")]
                origin: PersistentQueryOrigin,
                #[serde(with = "verified_final")]
                verified_final: AtomicBool,
                extra: QueryRevisionsExtra,
            }

            let revisions = DeserializeQueryRevisions::deserialize(deserializer)?;

            Ok(QueryRevisions {
                changed_at: revisions.changed_at,
                durability: revisions.durability,
                origin_and_extra: OriginAndExtra::new(revisions.origin, revisions.extra),
                #[cfg(feature = "accumulator")]
                accumulated_inputs: Default::default(), // TODO: Support serializing accumulators
                verified_final: revisions.verified_final,
            })
        }
    }

    // A workaround the fact that `shuttle` atomic types do not implement `serde::{Serialize, Deserialize}`.
    pub(super) mod verified_final {
        use crate::sync::atomic::{AtomicBool, Ordering};

        pub fn serialize<S>(value: &AtomicBool, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serde::Serialize::serialize(&value.load(Ordering::Acquire), serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<AtomicBool, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            serde::Deserialize::deserialize(deserializer).map(AtomicBool::new)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    #[cfg(feature = "persistence")]
    use super::QueryEdgeKind;
    #[cfg(feature = "persistence")]
    use super::persistence::PersistentQueryOrigin;
    #[cfg(not(feature = "shuttle"))]
    use super::{
        AssignedOriginAndExtra, OutputOrder, QueryRevisionsExtra, QueryRevisionsExtraInner,
        SliceWithHeader,
    };
    use super::{OriginAndExtra, PackedQueryEdge, QueryEdge, QueryOriginRef};
    #[cfg(not(feature = "shuttle"))]
    use crate::Revision;
    use crate::{DatabaseKeyIndex, Id, IngredientIndex};

    #[test]
    fn query_origin_packs_edges_that_fit() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let other_input = QueryEdge::input(key(232, 10_842_123, 42));
        let origin = OriginAndExtra::derived([input, other_input].into_iter(), Default::default());
        let QueryOriginRef::Derived(edges) = origin.origin() else {
            panic!("expected derived origin");
        };

        assert!(edges.is_packed());
        assert_eq!(edges.allocation_size(), 2 * size_of::<PackedQueryEdge>());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![input, other_input]);
        assert_eq!(
            edges.iter().rev().collect::<Vec<_>>(),
            vec![other_input, input]
        );
        assert_eq!(
            origin.origin().inputs().collect::<Vec<_>>(),
            vec![input.key(), other_input.key()]
        );
        assert_eq!(origin.origin().outputs().collect::<Vec<_>>(), vec![]);
    }

    #[test]
    fn query_origin_spills_all_edges_if_it_contains_an_output() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let output = QueryEdge::output(key(232, 10_842_123, 42));
        let origin = OriginAndExtra::derived([input, output].into_iter(), Default::default());
        let QueryOriginRef::Derived(edges) = origin.origin() else {
            panic!("expected derived origin");
        };

        assert!(!edges.is_packed());
        assert_eq!(edges.allocation_size(), 2 * size_of::<QueryEdge>());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![input, output]);
        assert_eq!(
            origin.origin().inputs().collect::<Vec<_>>(),
            vec![input.key()]
        );
        assert_eq!(
            origin.origin().outputs().collect::<Vec<_>>(),
            vec![output.key()]
        );
    }

    #[test]
    fn query_origin_packs_largest_supported_generation() {
        let input = QueryEdge::input(key(231, 10_842_122, PackedQueryEdge::GENERATION_MASK));
        let origin = OriginAndExtra::derived([input].into_iter(), Default::default());
        let QueryOriginRef::Derived(edges) = origin.origin() else {
            panic!("expected derived origin");
        };

        assert!(edges.is_packed());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![input]);
    }

    #[test]
    fn query_origin_spills_all_edges_if_generation_does_not_fit() {
        let packed = QueryEdge::input(key(231, 10_842_122, 41));
        let wide = QueryEdge::input(key(232, 10_842_123, PackedQueryEdge::GENERATION_MASK + 1));
        let origin = OriginAndExtra::derived([packed, wide].into_iter(), Default::default());
        let QueryOriginRef::Derived(edges) = origin.origin() else {
            panic!("expected derived origin");
        };

        assert!(!edges.is_packed());
        assert_eq!(edges.allocation_size(), 2 * size_of::<QueryEdge>());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![packed, wide]);
    }

    #[test]
    fn query_origin_spills_if_ingredient_does_not_fit() {
        let wide = QueryEdge::input(key(PackedQueryEdge::INGREDIENT_MASK + 1, 10_842_122, 41));
        let origin = OriginAndExtra::derived([wide].into_iter(), Default::default());
        let QueryOriginRef::Derived(edges) = origin.origin() else {
            panic!("expected derived origin");
        };

        assert!(!edges.is_packed());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![wide]);
    }

    #[test]
    fn output_scan_work_follows_the_retained_edge_representation() {
        let first = QueryEdge::input(key(231, 10_842_122, 41));
        let second = QueryEdge::input(key(232, 10_842_123, 42));
        let wide = QueryEdge::input(key(233, 10_842_124, PackedQueryEdge::GENERATION_MASK + 1));
        let output = QueryEdge::output(key(234, 10_842_125, 43));

        assert_eq!(QueryOriginRef::Assigned(first.key()).output_scan_work(), 0);
        let packed = OriginAndExtra::derived([first, second].into_iter(), Default::default());
        assert_eq!(packed.origin().output_scan_work(), 0);
        let packed_untracked =
            OriginAndExtra::derived_untracked([first, second].into_iter(), Default::default());
        assert_eq!(packed_untracked.origin().output_scan_work(), 0);
        let inputs = OriginAndExtra::derived([first, second, wide].into_iter(), Default::default());
        assert!(!inputs.origin().edges().is_packed());
        assert_eq!(inputs.origin().output_scan_work(), 3);
        assert!(inputs.origin().outputs().next().is_none());
        let outputs =
            OriginAndExtra::derived([first, second, output].into_iter(), Default::default());
        assert!(!outputs.origin().edges().is_packed());
        assert_eq!(outputs.origin().output_scan_work(), 3);
        assert_eq!(outputs.origin().outputs().next(), Some(output.key()));
        let untracked = OriginAndExtra::derived_untracked(
            [first, second, wide].into_iter(),
            Default::default(),
        );
        assert_eq!(untracked.origin().output_scan_work(), 3);
        assert!(untracked.origin().outputs().next().is_none());
    }

    #[cfg(all(not(feature = "persistence"), not(feature = "shuttle")))]
    #[test]
    fn clearing_edges_preserves_co_allocated_extra() {
        let packed = QueryEdge::input(key(231, 10_842_122, 41));
        let wide = QueryEdge::input(key(PackedQueryEdge::INGREDIENT_MASK + 1, 10_842_123, 42));

        for edge in [packed, wide] {
            let mut extra = QueryRevisionsExtraInner::empty();
            extra.cycle_converged = true;
            extra.execution_revision = Some(Revision::start());
            extra.output_order = OutputOrder::Inherited;
            let mut origin = OriginAndExtra::derived_untracked(
                [edge].into_iter(),
                QueryRevisionsExtra(Some(extra)),
            );

            origin.clear_edges();

            assert_eq!(origin.extra().unwrap().output_order, OutputOrder::Inherited);
            assert!(origin.extra().unwrap().cycle_converged);
            assert_eq!(
                origin.extra().unwrap().execution_revision,
                Some(Revision::start())
            );
            let QueryOriginRef::DerivedUntracked(edges) = origin.origin() else {
                panic!("expected untracked derived origin");
            };
            assert!(edges.is_packed());
            assert!(edges.iter().next().is_none());
            assert_eq!(
                origin.allocation_size(),
                SliceWithHeader::<QueryRevisionsExtraInner, PackedQueryEdge>::layout(0)
                    .0
                    .size()
            );
        }
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn stored_derived_origin_inlines_extra_before_packed_edges() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let other_input = QueryEdge::input(key(232, 10_842_123, 42));
        let mut origin =
            OriginAndExtra::derived([input, other_input].into_iter(), Default::default());

        let extra = origin.get_or_insert_extra();
        assert_eq!(extra.output_order, OutputOrder::Execution);
        extra.output_order = OutputOrder::Inherited;
        origin.get_or_insert_extra().cycle_converged = true;
        assert_eq!(origin.extra().unwrap().output_order, OutputOrder::Inherited);

        assert_eq!(
            origin.allocation_size(),
            SliceWithHeader::<QueryRevisionsExtraInner, PackedQueryEdge>::layout(2)
                .0
                .size()
        );
        assert!(origin.extra().unwrap().cycle_converged);
        assert_eq!(
            origin.origin().edges().iter().collect::<Vec<_>>(),
            vec![input, other_input]
        );
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn stored_derived_origin_builds_directly_with_extra() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let mut extra = QueryRevisionsExtraInner::empty();
        extra.cycle_converged = true;

        let origin = OriginAndExtra::derived([input].into_iter(), QueryRevisionsExtra(Some(extra)));

        assert!(origin.extra().unwrap().cycle_converged);
        assert_eq!(
            origin.origin().edges().iter().collect::<Vec<_>>(),
            vec![input]
        );
        assert_eq!(
            origin.allocation_size(),
            SliceWithHeader::<QueryRevisionsExtraInner, PackedQueryEdge>::layout(1)
                .0
                .size()
        );
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn stored_wide_origin_builds_directly_with_extra() {
        let input = QueryEdge::input(key(PackedQueryEdge::INGREDIENT_MASK + 1, 10_842_122, 41));
        let origin = OriginAndExtra::derived(
            [input].into_iter(),
            QueryRevisionsExtra(Some(QueryRevisionsExtraInner::empty())),
        );

        assert_eq!(
            origin.origin().edges().iter().collect::<Vec<_>>(),
            vec![input]
        );
        assert_eq!(
            origin.allocation_size(),
            SliceWithHeader::<QueryRevisionsExtraInner, QueryEdge>::layout(1)
                .0
                .size()
        );
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn assigned_origin_with_extra_keeps_the_key() {
        let key = key(231, 10_842_122, 41);
        let mut origin = OriginAndExtra::assigned(key);
        origin.get_or_insert_extra().cycle_converged = true;

        assert_eq!(
            origin.allocation_size(),
            size_of::<AssignedOriginAndExtra>()
        );
        assert!(origin.extra().unwrap().cycle_converged);
        let QueryOriginRef::Assigned(stored_key) = origin.origin() else {
            panic!("expected assigned origin");
        };
        assert_eq!(stored_key, key);
    }

    #[cfg(feature = "persistence")]
    #[test]
    fn query_origin_serde_round_trip_preserves_edges() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let output = QueryEdge::output(key(232, 10_842_123, PackedQueryEdge::GENERATION_MASK + 1));
        let origin = PersistentQueryOrigin::derived_untracked([input, output]);
        let serialized = serde_json::to_string(&origin).unwrap();
        let deserialized_origin: PersistentQueryOrigin = serde_json::from_str(&serialized).unwrap();
        let QueryOriginRef::DerivedUntracked(edges) = deserialized_origin.as_ref() else {
            panic!("expected untracked derived origin");
        };

        assert!(!edges.is_packed());
        assert_eq!(edges.iter().collect::<Vec<_>>(), vec![input, output]);
        assert_eq!(
            edges.iter().map(QueryEdge::kind).collect::<Vec<_>>(),
            vec![QueryEdgeKind::Input, QueryEdgeKind::Output]
        );
    }

    #[cfg(all(feature = "persistence", not(feature = "shuttle")))]
    #[test]
    fn restored_revisions_preserve_dependencies_without_execution_identity() {
        let input = QueryEdge::input(key(231, 10_842_122, 41));
        let output = QueryEdge::output(key(232, 10_842_123, 42));
        for output_order in [OutputOrder::Execution, OutputOrder::Inherited] {
            let mut extra = QueryRevisionsExtraInner::empty();
            extra.execution_revision = Some(Revision::start());
            extra.output_order = output_order;
            let revisions = super::QueryRevisions {
                changed_at: Revision::start(),
                durability: crate::Durability::HIGH,
                origin_and_extra: OriginAndExtra::derived(
                    [input, output].into_iter(),
                    QueryRevisionsExtra(Some(extra)),
                ),
                #[cfg(feature = "accumulator")]
                accumulated_inputs: Default::default(),
                verified_final: crate::sync::atomic::AtomicBool::new(true),
            };
            let serialized = serde_json::to_value(
                revisions.with_origin(PersistentQueryOrigin::derived([input, output])),
            )
            .unwrap();
            assert!(serialized["extra"].is_object());
            assert!(serialized["extra"].get("execution_revision").is_none());
            assert_eq!(
                serialized["extra"]["output_order"],
                serde_json::to_value(output_order).unwrap()
            );

            // Older extra records omit output_order and cannot establish causal output positions.
            // Decode that missing field as Inherited while preserving the remaining metadata.
            let mut legacy = serialized.clone();
            assert!(
                legacy["extra"]
                    .as_object_mut()
                    .unwrap()
                    .remove("output_order")
                    .is_some()
            );
            for (serialized, expected_order) in
                [(serialized, output_order), (legacy, OutputOrder::Inherited)]
            {
                let restored: super::QueryRevisions = serde_json::from_value(serialized).unwrap();
                assert_eq!(restored.output_order(), expected_order);
                assert_eq!(restored.execution_revision(), None);
                assert_eq!(restored.changed_at, revisions.changed_at);
                assert_eq!(restored.durability, revisions.durability);
                assert_eq!(
                    restored.origin().inputs().collect::<Vec<_>>(),
                    vec![input.key()]
                );
                assert_eq!(
                    restored.origin().outputs().collect::<Vec<_>>(),
                    vec![output.key()]
                );
                assert!(
                    restored
                        .verified_final
                        .load(crate::sync::atomic::Ordering::Acquire)
                );
            }
        }
    }

    fn key(ingredient: u32, index: u32, generation: u32) -> DatabaseKeyIndex {
        // SAFETY: Test inputs are valid `Id` indices.
        let id = unsafe { Id::from_index(index) }.with_generation(generation);

        DatabaseKeyIndex::new(IngredientIndex::new(ingredient), id)
    }
}
