//! A tracked source entry that declares its originating definition before constructing a builder.
//!
//! Parsing, indexing and resolver preparation remain tracked inputs outside the semantic allowance.
//! Completed results own their inference data; pending dependencies never export a provisional type.

mod tests;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::definition::Definition;

use super::Router;
use crate::types::callable::scheduled_probe::{Boundary, Key, run_with};
use crate::types::infer::DefinitionInference;
use crate::{Db, ProgramEnvironment};

thread_local! {
    static CANCEL_NEXT_ROOT: RefCell<Option<(usize, salsa::CancellationToken)>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::SalsaValue)]
struct RootPolicy {
    allowance: usize,
    reverse_execution: bool,
    reverse_merge: bool,
}

#[salsa::interned(debug)]
struct DefinitionRootRequest<'db> {
    #[returns(copy)]
    definition: Definition<'db>,
    #[returns(copy)]
    policy: RootPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum Incomplete {
    Allowance,
    Source(Boundary),
    DependencyCycle,
    Pending,
}

#[derive(Clone, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum Completion<'db> {
    Complete(Arc<DefinitionInference<'db>>),
    Incomplete(Incomplete),
}

#[derive(Clone, Debug, Eq, PartialEq, salsa::SalsaValue)]
struct DefinitionRootOutcome<'db> {
    root: Completion<'db>,
    completed: FxHashMap<Definition<'db>, Arc<DefinitionInference<'db>>>,
    boundaries: FxHashMap<Definition<'db>, Boundary>,
    pending: FxHashSet<Definition<'db>>,
    pending_edges: FxHashSet<(Definition<'db>, Definition<'db>)>,
    definition_polls: FxHashMap<Definition<'db>, usize>,
    definition_starts: FxHashMap<Definition<'db>, usize>,
    source_work_polls: usize,
    work: usize,
}

impl DefinitionRootOutcome<'_> {
    fn empty(reason: Incomplete) -> Self {
        Self {
            root: Completion::Incomplete(reason),
            completed: FxHashMap::default(),
            boundaries: FxHashMap::default(),
            pending: FxHashSet::default(),
            pending_edges: FxHashSet::default(),
            definition_polls: FxHashMap::default(),
            definition_starts: FxHashMap::default(),
            source_work_polls: 0,
            work: 0,
        }
    }
}

// Every admitted producer poll registers at most one dependency before suspending. For P polls,
// the combined definition/checkpoint record count N and demand-edge count E are each at most P.
// Snapshot/output construction and destruction take at most 12N record visits/copies; pending
// edge export, cycle adjacency construction and queue processing take at most 12E edge operations.
// Poll statistics and root-answer copies add at most 4P + 8 units. Thus 32P + 8 conservatively
// bounds transport, and P is bounded by semantic work. Hash-table operations and Arc clones are
// logical units, not allocator-time bounds. The request identity is retained even at allowance 0.
const FIXED_TRANSPORT: usize = 8;
const TRANSPORT_PER_SEMANTIC_UNIT: usize = 32;

#[salsa::tracked(returns(ref))]
fn evaluate_definition_root<'db>(
    db: &'db dyn Db,
    request: DefinitionRootRequest<'db>,
) -> DefinitionRootOutcome<'db> {
    let definition = request.definition(db);
    let policy = request.policy(db);
    let Some(available) = policy.allowance.checked_sub(FIXED_TRANSPORT) else {
        let mut outcome = DefinitionRootOutcome::empty(Incomplete::Allowance);
        outcome.pending.insert(definition);
        return outcome;
    };
    let semantic_allowance = available / (1 + TRANSPORT_PER_SEMANTIC_UNIT);
    let env = ProgramEnvironment::from_definition(definition);
    let router = Router::default();
    router.cancellation_probe.replace(CANCEL_NEXT_ROOT.take());
    let result = run_with(
        db,
        &env,
        &router,
        semantic_allowance,
        policy.reverse_execution,
        policy.reverse_merge,
        |router| async { router.consumer_definition_demand(definition).await },
    );
    let mut outcome = DefinitionRootOutcome::empty(Incomplete::Pending);
    outcome.work = FIXED_TRANSPORT + router.logical_work.get() * (1 + TRANSPORT_PER_SEMANTIC_UNIT);

    // The committed table is the completion certificate, including when a later operation hits
    // an outer boundary. Keep independently completed definitions in either case.
    let definitions = router.definitions.borrow();
    #[expect(
        clippy::iter_over_hash_type,
        reason = "each definition is copied independently into unordered output maps and sets"
    )]
    for (owner, entry) in &*definitions {
        match &entry.answer {
            Some(Ok(inference)) => {
                outcome.completed.insert(*owner, Arc::clone(inference));
            }
            Some(Err(boundary)) => {
                outcome.boundaries.insert(*owner, *boundary);
            }
            None => {
                outcome.pending.insert(*owner);
            }
        }
    }
    #[expect(
        clippy::iter_over_hash_type,
        reason = "pending dependency edges are exported as a set with no observable traversal order"
    )]
    for child in &outcome.pending {
        for dependent in &definitions[child].dependents {
            if let Key::Definition(parent) = dependent
                && outcome.pending.contains(parent)
            {
                outcome.pending_edges.insert((*parent, *child));
            }
        }
    }
    let incomplete = match result {
        Ok(snapshot) => {
            outcome.definition_polls = snapshot.definition_polls;
            outcome.definition_starts = snapshot.definition_starts;
            outcome.source_work_polls = snapshot.source_work_polls.values().sum();
            if snapshot.graph.exhausted {
                Incomplete::Allowance
            } else if has_dependency_cycle(&outcome.pending, &outcome.pending_edges) {
                Incomplete::DependencyCycle
            } else {
                Incomplete::Pending
            }
        }
        Err(boundary) => Incomplete::Source(boundary),
    };
    outcome.root = if let Some(inference) = outcome.completed.get(&definition) {
        Completion::Complete(Arc::clone(inference))
    } else if let Some(boundary) = outcome.boundaries.get(&definition) {
        Completion::Incomplete(Incomplete::Source(*boundary))
    } else {
        outcome.pending.insert(definition);
        Completion::Incomplete(incomplete)
    };
    db.unwind_if_revision_cancelled();
    outcome
}

/// Removing vertices with no incoming edge leaves a vertex exactly when the pending graph cycles.
fn has_dependency_cycle<'db>(
    pending: &FxHashSet<Definition<'db>>,
    edges: &FxHashSet<(Definition<'db>, Definition<'db>)>,
) -> bool {
    let mut incoming: FxHashMap<_, usize> = pending.iter().map(|owner| (*owner, 0)).collect();
    let mut outgoing: FxHashMap<_, Vec<_>> = FxHashMap::default();
    #[expect(
        clippy::iter_over_hash_type,
        reason = "cycle detection depends on edge counts and vertex removal, not adjacency traversal order"
    )]
    for (parent, child) in edges {
        *incoming.entry(*child).or_default() += 1;
        outgoing.entry(*parent).or_default().push(*child);
    }
    let mut ready: VecDeque<_> = incoming
        .iter()
        .filter_map(|(owner, count)| (*count == 0).then_some(*owner))
        .collect();
    let mut removed = 0;
    while let Some(owner) = ready.pop_front() {
        removed += 1;
        for child in outgoing.get(&owner).into_iter().flatten() {
            if let Some(count) = incoming.get_mut(child) {
                *count -= 1;
                if *count == 0 {
                    ready.push_back(*child);
                }
            }
        }
    }
    removed != pending.len()
}
