use std::{fmt, mem, ops};

use crate::Revision;
#[cfg(feature = "accumulator")]
use crate::accumulator::{
    Accumulator,
    accumulated_map::{AccumulatedMap, AtomicInputAccumulatedValues, InputAccumulatedValues},
};
use crate::attempt_probe::AttemptSupport;
#[cfg(all(test, not(feature = "shuttle")))]
use crate::attempt_probe::transfer_test_support::{self as transfer_trace, Event, Kind};
use crate::hash::FxIndexSet;
use crate::key::DatabaseKeyIndex;
use crate::runtime::Stamp;
use crate::sync::atomic::AtomicBool;
use crate::tracked_struct::{Disambiguator, DisambiguatorMap, IdentityHash, IdentityMap};
use crate::zalsa::Zalsa;
use crate::zalsa_local::{
    OriginAndExtra, OutputOrder, QueryEdge, QueryRevisions, QueryRevisionsExtra,
};
use crate::{
    Id,
    cycle::{CycleHeads, IterationStamp},
};
use crate::{durability::Durability, tracked_struct::Identity};

pub(crate) mod read_storage;

#[derive(Debug)]
pub(crate) struct ActiveQuery {
    pub(crate) attempt_policy: crate::attempt_probe::OperationPolicy,
    attempt_support: Option<AttemptSupport>,
    /// What query is executing
    pub(crate) database_key_index: DatabaseKeyIndex,

    /// Minimum durability of inputs observed so far.
    durability: Durability,

    /// Maximum revision of all inputs observed. If we observe an
    /// untracked read, this will be set to the most recent revision.
    changed_at: Revision,

    /// Inputs: Set of subqueries that were accessed thus far.
    /// Outputs: Tracks values written by this query. Could be...
    ///
    /// * invocations of `specify`
    /// * accumulators pushed to
    input_outputs: FxIndexSet<QueryEdge>,

    output_order: OutputOrder,

    /// True if there was an untracked read.
    untracked_read: bool,

    /// When new tracked structs are created, their data is hashed, and the resulting
    /// hash is added to this map. If it is not present, then the disambiguator is 0.
    /// Otherwise it is 1 more than the current value (which is incremented).
    ///
    /// This table starts empty as the query begins and is gradually populated.
    /// Note that if a query executes in 2 different revisions but creates the same
    /// set of tracked structs, they will get the same disambiguator values.
    disambiguator_map: DisambiguatorMap,

    /// Map from tracked struct keys (which include the hash + disambiguator) to their
    /// final id.
    tracked_struct_ids: IdentityMap,

    /// Stores the values accumulated to the given ingredient.
    /// The type of accumulated value is erased but known to the ingredient.
    #[cfg(feature = "accumulator")]
    accumulated: AccumulatedMap,

    /// [`InputAccumulatedValues::Empty`] if any input read during the query's execution
    /// has any accumulated values.
    #[cfg(feature = "accumulator")]
    accumulated_inputs: InputAccumulatedValues,

    /// Provisional cycle results that this query depends on.
    cycle_heads: CycleHeads,
}

impl ActiveQuery {
    pub(crate) fn assert_attempt_support_allowed(
        &self,
        zalsa: &Zalsa,
        support: &AttemptSupport,
        incomplete: bool,
    ) {
        assert!(
            self.attempt_policy.allows_incomplete()
                || (self.attempt_policy.declared
                    == crate::attempt_probe::QueryPolicy::CompleteOnly
                    && !incomplete
                    && support.is_running_current(zalsa))
        );
        if let Some(previous) = &self.attempt_support {
            assert!(previous.same_owner(support), "query mixed attempt owners");
        }
    }

    pub(crate) fn add_attempt_support(
        &mut self,
        zalsa: &Zalsa,
        support: &AttemptSupport,
        incomplete: bool,
    ) {
        self.assert_attempt_support_allowed(zalsa, support, incomplete);
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = Event::new(Kind::SupportIncoming).key(self.database_key_index);
            event.previous_support = self
                .attempt_support
                .as_ref()
                .map(transfer_trace::support_snapshot);
            event.support = Some(transfer_trace::support_snapshot(support));
            event.decision = incomplete;
            transfer_trace::record(event);
        }
        if self.attempt_support.is_none() {
            self.attempt_support = Some(support.clone());
        }
        if incomplete {
            if let Some(support) = &mut self.attempt_support {
                support.make_incomplete();
            }
        }
        #[cfg(all(test, not(feature = "shuttle")))]
        {
            let mut event = Event::new(Kind::SupportAccepted).key(self.database_key_index);
            event.support = self
                .attempt_support
                .as_ref()
                .map(transfer_trace::support_snapshot);
            transfer_trace::record(event);
        }
    }

    pub(crate) fn attempt_support(&self) -> Option<&AttemptSupport> {
        self.attempt_support.as_ref()
    }

    pub(crate) fn attempt_incomplete(&self) -> bool {
        self.attempt_support
            .as_ref()
            .is_some_and(|support| support.incomplete(true))
    }

    pub(crate) fn finish_attempt_incomplete(&mut self) {
        assert!(self.attempt_policy.allows_incomplete());
        assert!(self.attempt_incomplete());
        assert!(
            self.tracked_struct_ids.is_empty(),
            "incomplete query owns tracked structs"
        );
        assert!(
            self.input_outputs
                .iter()
                .all(|edge| edge.kind() == crate::zalsa_local::QueryEdgeKind::Input),
            "incomplete query owns outputs"
        );
        #[cfg(feature = "accumulator")]
        assert!(
            self.accumulated.is_empty(),
            "incomplete query owns accumulators"
        );
        if let Some(support) = &mut self.attempt_support {
            support.make_incomplete();
        }
        self.cycle_heads = CycleHeads::default();
    }

    pub(super) fn seed_iteration(
        &mut self,
        durability: Durability,
        changed_at: Revision,
        edges: crate::zalsa_local::QueryEdges<'_>,
        untracked_read: bool,
        active_tracked_ids: &[(Identity, Id)],
    ) {
        self.seed_output_ownership(edges, active_tracked_ids);
        self.durability = self.durability.min(durability);
        self.changed_at = self.changed_at.max(changed_at);
        self.untracked_read |= untracked_read;
    }

    pub(super) fn seed_output_ownership(
        &mut self,
        edges: crate::zalsa_local::QueryEdges<'_>,
        active_tracked_ids: &[(Identity, Id)],
    ) {
        assert!(self.input_outputs.is_empty());

        // Copy over outputs for `diff_outputs` to retain ownership until stale-output cleanup;
        // don't copy inputs because cycle heads flatten all input dependencies. The outputs'
        // earlier creation prerequisites are not recorded here, so these output positions
        // cannot justify ordered validation.
        for edge in edges.iter_outputs() {
            self.output_order = OutputOrder::Inherited;
            self.input_outputs.insert(edge);
        }

        // Mark all tracked structs from the previous iteration as active and reserve
        // their identities so that newly created structs receive later disambiguators.
        self.tracked_struct_ids
            .mark_all_active(active_tracked_ids.iter().copied());
        self.disambiguator_map
            .seed(active_tracked_ids.iter().map(|(id, _)| id));
    }

    pub(super) fn take_cycle_heads(&mut self) -> CycleHeads {
        std::mem::take(&mut self.cycle_heads)
    }

    pub(crate) fn cycle_heads(&self) -> &CycleHeads {
        &self.cycle_heads
    }

    #[cfg(test)]
    pub(crate) fn completion_state(&self) -> (&FxIndexSet<QueryEdge>, bool) {
        (&self.input_outputs, self.attempt_support.is_some())
    }

    pub(crate) fn detach_input_outputs(&mut self) -> DetachedInputOutputs {
        DetachedInputOutputs(std::mem::take(&mut self.input_outputs))
    }

    pub(super) fn add_read(
        &mut self,
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
        cycle_heads: &CycleHeads,
        #[cfg(feature = "accumulator")] has_accumulated: bool,
        #[cfg(feature = "accumulator")] accumulated_inputs: &AtomicInputAccumulatedValues,
    ) {
        #[cfg(feature = "accumulator")]
        let accumulated_inputs = match has_accumulated {
            true => InputAccumulatedValues::Any,
            false => accumulated_inputs.load(),
        };

        self.add_read_observed(
            input,
            durability,
            changed_at,
            cycle_heads,
            #[cfg(feature = "accumulator")]
            accumulated_inputs,
        );
    }

    pub(crate) fn read_records_input(
        durability: Durability,
        cycle_heads: &CycleHeads,
        #[cfg(feature = "accumulator")] accumulated_inputs: InputAccumulatedValues,
    ) -> bool {
        let record_input = cfg!(feature = "persistence")
            || durability != Durability::NEVER_CHANGE
            || !cycle_heads.is_empty();
        #[cfg(feature = "accumulator")]
        let record_input = record_input || accumulated_inputs.is_any();

        record_input
    }

    pub(crate) fn add_read_observed(
        &mut self,
        input: DatabaseKeyIndex,
        durability: Durability,
        changed_at: Revision,
        cycle_heads: &CycleHeads,
        #[cfg(feature = "accumulator")] accumulated_inputs: InputAccumulatedValues,
    ) {
        self.durability = self.durability.min(durability);
        self.changed_at = self.changed_at.max(changed_at);

        if Self::read_records_input(
            durability,
            cycle_heads,
            #[cfg(feature = "accumulator")]
            accumulated_inputs,
        ) {
            self.input_outputs.insert(QueryEdge::input(input));
        }

        self.cycle_heads.extend(cycle_heads);
        #[cfg(feature = "accumulator")]
        {
            self.accumulated_inputs |= accumulated_inputs;
        }
    }

    pub(super) fn add_read_simple(
        &mut self,
        input: DatabaseKeyIndex,
        durability: Durability,
        revision: Revision,
    ) {
        self.durability = self.durability.min(durability);
        self.add_changed_at(revision);

        if cfg!(feature = "persistence") || durability != Durability::NEVER_CHANGE {
            self.input_outputs.insert(QueryEdge::input(input));
        }
    }

    pub(super) fn add_changed_at(&mut self, revision: Revision) {
        self.changed_at = self.changed_at.max(revision);
    }

    pub(super) fn add_untracked_read(&mut self, changed_at: Revision) {
        self.untracked_read = true;
        self.durability = Durability::MIN;
        self.changed_at = changed_at;
    }

    #[cfg(feature = "accumulator")]
    pub(super) fn accumulate(&mut self, index: crate::IngredientIndex, value: impl Accumulator) {
        self.accumulated.accumulate(index, value);
    }

    /// Adds a key to our list of outputs, returning whether it was newly inserted.
    pub(super) fn add_output(&mut self, key: DatabaseKeyIndex) -> bool {
        self.input_outputs.insert(QueryEdge::output(key))
    }

    /// True if the given key was output by this query.
    pub(super) fn disambiguate(&mut self, key: IdentityHash) -> Disambiguator {
        self.disambiguator_map.disambiguate(key)
    }

    pub(super) fn stamp(&self) -> Stamp {
        Stamp {
            durability: self.durability,
            changed_at: self.changed_at,
        }
    }

    pub(crate) fn tracked_struct_ids(&self) -> &IdentityMap {
        &self.tracked_struct_ids
    }

    pub(crate) fn tracked_struct_ids_mut(&mut self) -> &mut IdentityMap {
        &mut self.tracked_struct_ids
    }
}

impl ActiveQuery {
    fn new(database_key_index: DatabaseKeyIndex) -> Self {
        ActiveQuery {
            attempt_policy: crate::attempt_probe::current_operation_policy(),
            attempt_support: None,
            database_key_index,
            durability: Durability::MAX,
            changed_at: Revision::start(),
            input_outputs: FxIndexSet::default(),
            output_order: OutputOrder::Execution,
            untracked_read: false,
            disambiguator_map: Default::default(),
            tracked_struct_ids: Default::default(),
            cycle_heads: Default::default(),
            #[cfg(feature = "accumulator")]
            accumulated: Default::default(),
            #[cfg(feature = "accumulator")]
            accumulated_inputs: Default::default(),
        }
    }

    fn prepare_completion(
        &mut self,
        iteration: IterationStamp,
        force_extra: bool,
    ) -> QueryCompletion {
        let &mut Self {
            attempt_policy: _,
            ref mut attempt_support,
            database_key_index: _,
            durability,
            changed_at,
            input_outputs: _,
            ref mut output_order,
            untracked_read,
            ref mut disambiguator_map,
            ref mut tracked_struct_ids,
            ref mut cycle_heads,
            #[cfg(feature = "accumulator")]
            ref mut accumulated,
            #[cfg(feature = "accumulator")]
            accumulated_inputs,
        } = self;

        disambiguator_map.clear();

        #[cfg(feature = "accumulator")]
        let accumulated_inputs = AtomicInputAccumulatedValues::new(accumulated_inputs);
        let verified_final = cycle_heads.is_empty();
        let (active_tracked_structs, stale_tracked_structs) = tracked_struct_ids.drain();

        let extra = QueryRevisionsExtra::new(
            #[cfg(feature = "accumulator")]
            mem::take(accumulated),
            active_tracked_structs,
            mem::take(cycle_heads),
            iteration,
            force_extra,
            attempt_support.take(),
            mem::replace(output_order, OutputOrder::Execution),
        );
        QueryCompletion {
            changed_at,
            durability,
            untracked_read,
            extra,
            #[cfg(feature = "accumulator")]
            accumulated_inputs,
            verified_final,
            stale_tracked_structs,
        }
    }

    fn clear(&mut self) {
        let Self {
            attempt_policy: _,
            attempt_support,
            database_key_index: _,
            durability: _,
            changed_at: _,
            input_outputs,
            output_order,
            untracked_read: _,
            disambiguator_map,
            tracked_struct_ids,
            cycle_heads,
            #[cfg(feature = "accumulator")]
            accumulated,
            #[cfg(feature = "accumulator")]
                accumulated_inputs: _,
        } = self;
        input_outputs.clear();
        *output_order = OutputOrder::Execution;
        *attempt_support = None;
        disambiguator_map.clear();
        tracked_struct_ids.clear();
        *cycle_heads = Default::default();
        #[cfg(feature = "accumulator")]
        accumulated.clear();
    }

    fn reset_for(&mut self, new_database_key_index: DatabaseKeyIndex) {
        let Self {
            attempt_policy,
            attempt_support,
            database_key_index,
            durability,
            changed_at,
            input_outputs,
            output_order,
            untracked_read,
            disambiguator_map,
            tracked_struct_ids,
            cycle_heads,
            #[cfg(feature = "accumulator")]
            accumulated,
            #[cfg(feature = "accumulator")]
            accumulated_inputs,
        } = self;
        *database_key_index = new_database_key_index;
        *attempt_policy = crate::attempt_probe::current_operation_policy();
        *attempt_support = None;
        *durability = Durability::MAX;
        *changed_at = Revision::start();
        *untracked_read = false;
        *output_order = OutputOrder::Execution;
        debug_assert!(
            input_outputs.is_empty(),
            "`ActiveQuery::clear` or `ActiveQuery::into_revisions` should've been called"
        );
        debug_assert!(
            disambiguator_map.is_empty(),
            "`ActiveQuery::clear` or `ActiveQuery::into_revisions` should've been called"
        );
        debug_assert!(
            tracked_struct_ids.is_empty(),
            "`ActiveQuery::clear` or `ActiveQuery::into_revisions` should've been called"
        );
        debug_assert!(
            cycle_heads.is_empty(),
            "`ActiveQuery::clear` or `ActiveQuery::into_revisions` should've been called"
        );
        #[cfg(feature = "accumulator")]
        {
            *accumulated_inputs = Default::default();
            debug_assert!(
                accumulated.is_empty(),
                "`ActiveQuery::clear` or `ActiveQuery::into_revisions` should've been called"
            );
        }
    }
}

pub(crate) struct DetachedInputOutputs(FxIndexSet<QueryEdge>);

impl DetachedInputOutputs {
    pub(crate) fn input_outputs(&self) -> &FxIndexSet<QueryEdge> {
        &self.0
    }
}

#[derive(Default)]
pub(crate) struct QueryStack {
    stack: Vec<ActiveQuery>,
    len: usize,
}

impl std::fmt::Debug for QueryStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            f.debug_list()
                .entries(self.stack.iter().map(|q| q.database_key_index))
                .finish()
        } else {
            f.debug_struct("QueryStack")
                .field("stack", &self.stack)
                .field("len", &self.len)
                .finish()
        }
    }
}

impl ops::Deref for QueryStack {
    type Target = [ActiveQuery];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.stack[..self.len]
    }
}

impl ops::DerefMut for QueryStack {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stack[..self.len]
    }
}

impl QueryStack {
    pub(crate) fn push_new_query(&mut self, database_key_index: DatabaseKeyIndex) {
        if self.len < self.stack.len() {
            self.stack[self.len].reset_for(database_key_index);
        } else {
            self.stack.push(ActiveQuery::new(database_key_index));
        }
        self.len += 1;
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn pop_into_revisions(
        &mut self,
        key: DatabaseKeyIndex,
        iteration: IterationStamp,
        #[cfg(debug_assertions)] push_len: usize,
    ) -> CompletedQuery {
        let active_query = self.pop_active_query(
            key,
            #[cfg(debug_assertions)]
            push_len,
        );
        let completion = active_query.prepare_completion(iteration, false);
        completion.finish(active_query.input_outputs.drain(..))
    }

    pub(crate) fn pop_detached_completion(
        &mut self,
        key: DatabaseKeyIndex,
        iteration: IterationStamp,
        detached_input_outputs: DetachedInputOutputs,
        force_extra: bool,
        #[cfg(debug_assertions)] push_len: usize,
    ) -> QueryCompletion {
        let active_query = self.pop_active_query(
            key,
            #[cfg(debug_assertions)]
            push_len,
        );
        let completion = active_query.prepare_completion(iteration, force_extra);
        let DetachedInputOutputs(mut reusable_frame_input_outputs) = detached_input_outputs;
        reusable_frame_input_outputs.clear();
        active_query.input_outputs = reusable_frame_input_outputs;
        completion
    }

    pub(crate) fn pop(&mut self, key: DatabaseKeyIndex, #[cfg(debug_assertions)] push_len: usize) {
        self.pop_active_query(
            key,
            #[cfg(debug_assertions)]
            push_len,
        )
        .clear()
    }

    fn pop_active_query(
        &mut self,
        key: DatabaseKeyIndex,
        #[cfg(debug_assertions)] push_len: usize,
    ) -> &mut ActiveQuery {
        #[cfg(debug_assertions)]
        assert_eq!(push_len, self.len(), "unbalanced push/pop");
        debug_assert_ne!(self.len, 0, "too many pops");
        self.len -= 1;
        let active_query = &mut self.stack[self.len];
        debug_assert_eq!(active_query.database_key_index, key, "unbalanced push/pop");
        active_query
    }
}

pub(crate) struct QueryCompletion {
    changed_at: Revision,
    durability: Durability,
    untracked_read: bool,
    extra: QueryRevisionsExtra,
    #[cfg(feature = "accumulator")]
    accumulated_inputs: AtomicInputAccumulatedValues,
    verified_final: bool,
    stale_tracked_structs: Vec<(Identity, Id)>,
}

impl QueryCompletion {
    pub(crate) fn finish(
        self,
        input_outputs: impl ExactSizeIterator<Item = QueryEdge>,
    ) -> CompletedQuery {
        let origin_and_extra = if self.untracked_read {
            OriginAndExtra::derived_untracked(input_outputs, self.extra)
        } else {
            OriginAndExtra::derived(input_outputs, self.extra)
        };
        CompletedQuery {
            revisions: QueryRevisions {
                changed_at: self.changed_at,
                durability: self.durability,
                origin_and_extra,
                #[cfg(feature = "accumulator")]
                accumulated_inputs: self.accumulated_inputs,
                verified_final: AtomicBool::new(self.verified_final),
            },
            stale_tracked_structs: self.stale_tracked_structs,
        }
    }
}

/// The state of a completed query.
pub(crate) struct CompletedQuery {
    /// Inputs and outputs accumulated during query execution.
    pub(crate) revisions: QueryRevisions,

    /// The keys of any tracked structs that were created in a previous execution of the
    /// query but not the current one, and should be marked as stale.
    pub(crate) stale_tracked_structs: Vec<(Identity, Id)>,
}

struct CapturedQuery {
    database_key_index: DatabaseKeyIndex,
    durability: Durability,
    changed_at: Revision,
    cycle_heads: CycleHeads,
}

impl fmt::Debug for CapturedQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug_struct = f.debug_struct("CapturedQuery");
        debug_struct
            .field("database_key_index", &self.database_key_index)
            .field("durability", &self.durability)
            .field("changed_at", &self.changed_at);
        if !self.cycle_heads.is_empty() {
            debug_struct.field("cycle_heads", &self.cycle_heads);
        }
        debug_struct.finish()
    }
}

pub struct Backtrace(Box<[CapturedQuery]>);

impl Backtrace {
    pub fn capture() -> Option<Self> {
        crate::with_attached_database(|db| {
            db.zalsa_local().try_with_query_stack(|stack| {
                Backtrace(
                    stack
                        .iter()
                        .rev()
                        .map(|query| CapturedQuery {
                            database_key_index: query.database_key_index,
                            durability: query.durability,
                            changed_at: query.changed_at,
                            cycle_heads: query.cycle_heads.clone(),
                        })
                        .collect(),
                )
            })
        })?
    }
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Backtrace ")?;

        let mut dbg = fmt.debug_list();

        for frame in &self.0 {
            dbg.entry(&frame);
        }

        dbg.finish()
    }
}

impl fmt::Display for Backtrace {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "query stacktrace:")?;
        let full = fmt.alternate();
        let indent = "             ";
        for (
            idx,
            &CapturedQuery {
                database_key_index,
                durability,
                changed_at,
                ref cycle_heads,
            },
        ) in self.0.iter().enumerate()
        {
            write!(fmt, "{idx:>4}: {database_key_index:?}")?;
            if full {
                write!(fmt, " -> ({changed_at:?}, {durability:#?}")?;
                write!(fmt, ")")?;
            }
            writeln!(fmt)?;
            crate::attach::with_attached_database(|db| {
                let ingredient = db
                    .zalsa()
                    .lookup_ingredient(database_key_index.ingredient_index());
                let loc = ingredient.location();
                writeln!(fmt, "{indent}at {}:{}", loc.file, loc.line)?;
                if !cycle_heads.is_empty() {
                    write!(fmt, "{indent}cycle heads: ")?;
                    for (idx, head) in cycle_heads.iter().enumerate() {
                        if idx != 0 {
                            write!(fmt, ", ")?;
                        }
                        write!(
                            fmt,
                            "{:?} -> iteration = {}",
                            head.database_key_index,
                            head.iteration.load().iteration()
                        )?;
                    }
                    writeln!(fmt)?;
                }
                Ok::<(), std::fmt::Error>(())
            })
            .transpose()?;
        }
        Ok(())
    }
}
