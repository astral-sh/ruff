use std::cmp::Ordering;
use std::io::Write;
use std::ops::ControlFlow;
use std::thread;

use ruff_index::Idx;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use super::{Operation, TddApply};
use crate::ProgramEnvironment;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::support::{Support, SupportId};
use crate::types::constraints::variables::{Constraint, ConstraintProvenance};
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintId, ConstraintSetStorage, InteriorNodeData, Node, NodeId,
    SourceOrder, SourceOrderId,
};
use crate::types::{BoundTypeVarInstance, Type, TypeVarVariance};

fn constraints<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    storage: &mut ConstraintSetStorage<'db>,
    count: i64,
) -> Vec<ConstraintId> {
    let typevar =
        BoundTypeVarInstance::synthetic(db, env, Name::new_static("T"), TypeVarVariance::Invariant);
    (0..count)
        .map(|index| {
            let Some(Ok(constraint)) = Constraint::new_lower_bound(
                db,
                ConstraintProvenance::Evidence,
                typevar,
                Type::int_literal(index),
            )
            .next() else {
                panic!("a literal lower bound supplies one satisfiable constraint");
            };
            storage.intern_constraint(db, env, constraint)
        })
        .collect()
}

// Each bit represents one assignment of the three independent propositions. Evaluate stored
// nodes in creation order, using Boolean operations rather than TDD operators or reductions.
fn truth_tables(storage: &ConstraintSetStorage<'_>) -> Vec<u8> {
    let mut tables = Vec::new();
    for data in &storage.nodes {
        let variable = [0b1010_1010, 0b1100_1100, 0b1111_0000][data.constraint.index()];
        let value = (variable & table(&tables, data.if_true))
            | table(&tables, data.if_uncertain)
            | (!variable & table(&tables, data.if_false));
        tables.push(value);
    }
    tables
}

fn table(tables: &[u8], node: NodeId) -> u8 {
    match node.node() {
        Node::AlwaysTrue => u8::MAX,
        Node::AlwaysFalse => 0,
        Node::Interior(_) => tables[node.index()],
    }
}

fn diagrams(storage: &mut ConstraintSetStorage<'_>, constraints: &[ConstraintId]) -> Vec<NodeId> {
    let mut diagrams = vec![ALWAYS_FALSE, ALWAYS_TRUE];
    for constraint in constraints {
        let children = diagrams.clone();
        // Vary all three branches, including uncertain branches that overlap guarded ones.
        for index in 0..64 {
            let len = children.len();
            diagrams.push(NodeId::with_uncertain(
                storage,
                *constraint,
                children[index % len],
                children[(index / 2 + 1) % len],
                children[(index / 4 + 3) % len],
            ));
        }
    }
    diagrams.sort_unstable_by_key(|node| node.0);
    diagrams.dedup();
    diagrams
}

#[test]
fn unions_match_boolean_truth_tables_with_cold_and_warm_caches() {
    let db = setup_db();
    let env = db.program_environment();
    let mut storage = ConstraintSetStorage::default();
    let constraints = constraints(&db, &env, &mut storage, 3);
    let diagrams = diagrams(&mut storage, &constraints);
    let initial = truth_tables(&storage);
    for left in &diagrams {
        for right in &diagrams {
            storage.or_cache.clear();
            let expected = table(&initial, *left) | table(&initial, *right);
            let forward = left.or(&mut storage, *right);
            let nodes = storage.nodes.len();
            assert_eq!(left.or(&mut storage, *right), forward);
            assert_eq!(storage.nodes.len(), nodes);
            let backward = right.or(&mut storage, *left);
            let actual = truth_tables(&storage);
            assert_eq!(table(&actual, forward), expected);
            assert_eq!(table(&actual, backward), expected);
        }
    }
}

#[test]
fn conjunction_negation_and_mixed_operations_match_boolean_truth_tables() {
    let db = setup_db();
    let env = db.program_environment();
    let mut storage = ConstraintSetStorage::default();
    let constraints = constraints(&db, &env, &mut storage, 3);
    let diagrams = diagrams(&mut storage, &constraints);
    let initial = truth_tables(&storage);
    // Sample partners across the fixture instead of multiplying three diagram collections.
    for (index, left) in diagrams.iter().copied().enumerate() {
        storage.negate_cache.clear();
        let complement = left.negate(&mut storage);
        let nodes = storage.nodes.len();
        assert_eq!(left.negate(&mut storage), complement);
        assert_eq!(storage.nodes.len(), nodes);
        assert_eq!(
            table(&truth_tables(&storage), complement),
            !table(&initial, left)
        );

        for offset in [0, 1, 7, 19] {
            let right = diagrams[(index + offset) % diagrams.len()];
            storage.and_cache.clear();
            storage.or_cache.clear();
            let intersection = left.and(&mut storage, right);
            let nodes = storage.nodes.len();
            assert_eq!(left.and(&mut storage, right), intersection);
            assert_eq!(storage.nodes.len(), nodes);
            let reverse = right.and(&mut storage, left);
            let union = left.or(&mut storage, right);
            let mixed = union.negate(&mut storage).or(&mut storage, intersection);
            let actual = truth_tables(&storage);
            let left_table = table(&initial, left);
            let right_table = table(&initial, right);
            assert_eq!(table(&actual, intersection), left_table & right_table);
            assert_eq!(table(&actual, reverse), left_table & right_table);
            assert_eq!(
                table(&actual, mixed),
                !(left_table | right_table) | (left_table & right_table),
            );
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StructuralState {
    nodes: Vec<InteriorNodeData>,
    supports: Vec<Support>,
    node_supports: Vec<SupportId>,
    source_orders: Vec<SourceOrder>,
    node_cache: FxHashMap<InteriorNodeData, NodeId>,
    source_order_cache: FxHashMap<SourceOrder, SourceOrderId>,
    and_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    or_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    negate_cache: FxHashMap<NodeId, NodeId>,
}

impl StructuralState {
    fn capture(storage: &ConstraintSetStorage<'_>) -> Self {
        Self {
            nodes: storage.nodes.raw.clone(),
            supports: storage.supports.raw.clone(),
            node_supports: storage.node_supports.raw.clone(),
            source_orders: storage.source_orders.raw.clone(),
            node_cache: storage.node_cache.clone(),
            source_order_cache: storage.source_order_cache.clone(),
            and_cache: storage.and_cache.clone(),
            or_cache: storage.or_cache.clone(),
            negate_cache: storage.negate_cache.clone(),
        }
    }
}

fn fork_storage<'db>(storage: &ConstraintSetStorage<'db>) -> ConstraintSetStorage<'db> {
    ConstraintSetStorage {
        compacted: storage.compacted.clone(),
        overlay_identity_state: storage.overlay_identity_state,
        constraints: storage.constraints.clone(),
        typevars: storage.typevars.clone(),
        nodes: storage.nodes.clone(),
        supports: storage.supports.clone(),
        constraint_supports: storage.constraint_supports.clone(),
        node_supports: storage.node_supports.clone(),
        source_orders: storage.source_orders.clone(),
        constraint_cache: storage.constraint_cache.clone(),
        typevar_cache: storage.typevar_cache.clone(),
        node_cache: storage.node_cache.clone(),
        source_order_cache: storage.source_order_cache.clone(),
        and_cache: storage.and_cache.clone(),
        or_cache: storage.or_cache.clone(),
        negate_cache: storage.negate_cache.clone(),
        ..ConstraintSetStorage::default()
    }
}

// This finite oracle retains the recursive algorithms independently of TddApply. It shares only
// node reduction and storage, allowing each completed cache insertion and arena ID to be compared.
struct RecursiveReference<'db> {
    storage: ConstraintSetStorage<'db>,
    publications: Vec<StructuralState>,
}

impl<'db> RecursiveReference<'db> {
    fn new(storage: &ConstraintSetStorage<'db>) -> Self {
        Self {
            storage: fork_storage(storage),
            publications: Vec::new(),
        }
    }

    fn apply(&mut self, operation: Operation) -> NodeId {
        match operation {
            Operation::And(left, right) => self.and(left, right),
            Operation::Or(left, right) => self.or(left, right),
            Operation::Negate(node) => self.negate(node),
        }
    }

    fn and(&mut self, left: NodeId, right: NodeId) -> NodeId {
        if left == right {
            return left;
        }
        match (left.node(), right.node()) {
            (Node::AlwaysFalse, _) | (_, Node::AlwaysFalse) => return ALWAYS_FALSE,
            (Node::AlwaysTrue, _) => return right,
            (_, Node::AlwaysTrue) => return left,
            (Node::Interior(_), Node::Interior(_)) => {}
        }
        let key = (left, right);
        if let Some(result) = self.storage.and_cache.get(&key) {
            return *result;
        }
        let left_data = self.storage.interior_node_data(left);
        let right_data = self.storage.interior_node_data(right);
        let (constraint, if_true, if_uncertain, if_false) = match left_data
            .constraint
            .ordering()
            .cmp(&right_data.constraint.ordering())
        {
            Ordering::Equal => {
                let other_if_true = self.or(right_data.if_true, right_data.if_uncertain);
                let true_from_true = self.and(left_data.if_true, other_if_true);
                let true_from_uncertain = self.and(left_data.if_uncertain, right_data.if_true);
                let if_true = self.or(true_from_true, true_from_uncertain);
                let if_uncertain = self.and(left_data.if_uncertain, right_data.if_uncertain);
                let other_if_false = self.or(right_data.if_uncertain, right_data.if_false);
                let false_from_false = self.and(left_data.if_false, other_if_false);
                let false_from_uncertain = self.and(left_data.if_uncertain, right_data.if_false);
                let if_false = self.or(false_from_false, false_from_uncertain);
                (left_data.constraint, if_true, if_uncertain, if_false)
            }
            Ordering::Less => (
                left_data.constraint,
                self.and(left_data.if_true, right),
                self.and(left_data.if_uncertain, right),
                self.and(left_data.if_false, right),
            ),
            Ordering::Greater => (
                right_data.constraint,
                self.and(left, right_data.if_true),
                self.and(left, right_data.if_uncertain),
                self.and(left, right_data.if_false),
            ),
        };
        let result = NodeId::with_uncertain(
            &mut self.storage,
            constraint,
            if_true,
            if_uncertain,
            if_false,
        );
        self.storage.and_cache.insert(key, result);
        self.publications
            .push(StructuralState::capture(&self.storage));
        result
    }

    fn or(&mut self, left: NodeId, right: NodeId) -> NodeId {
        match (left.node(), right.node()) {
            (Node::AlwaysTrue, _) | (_, Node::AlwaysTrue) => return ALWAYS_TRUE,
            (Node::AlwaysFalse, _) => return right,
            (_, Node::AlwaysFalse) => return left,
            (Node::Interior(_), Node::Interior(_)) => {}
        }
        let key = (left, right);
        if let Some(result) = self.storage.or_cache.get(&key) {
            return *result;
        }
        let left_data = self.storage.interior_node_data(left);
        let right_data = self.storage.interior_node_data(right);
        let (constraint, if_true, if_uncertain, if_false) = match left_data
            .constraint
            .ordering()
            .cmp(&right_data.constraint.ordering())
        {
            Ordering::Equal => (
                left_data.constraint,
                self.or(left_data.if_true, right_data.if_true),
                self.or(left_data.if_uncertain, right_data.if_uncertain),
                self.or(left_data.if_false, right_data.if_false),
            ),
            Ordering::Less => (
                left_data.constraint,
                left_data.if_true,
                self.or(left_data.if_uncertain, right),
                left_data.if_false,
            ),
            Ordering::Greater => (
                right_data.constraint,
                right_data.if_true,
                self.or(left, right_data.if_uncertain),
                right_data.if_false,
            ),
        };
        let result = NodeId::with_uncertain(
            &mut self.storage,
            constraint,
            if_true,
            if_uncertain,
            if_false,
        );
        self.storage.or_cache.insert(key, result);
        self.publications
            .push(StructuralState::capture(&self.storage));
        result
    }

    fn negate(&mut self, node: NodeId) -> NodeId {
        match node.node() {
            Node::AlwaysTrue => return ALWAYS_FALSE,
            Node::AlwaysFalse => return ALWAYS_TRUE,
            Node::Interior(_) => {}
        }
        if let Some(result) = self.storage.negate_cache.get(&node) {
            return *result;
        }
        let data = self.storage.interior_node_data(node);
        let not_true = self.negate(data.if_true);
        let not_uncertain = self.negate(data.if_uncertain);
        let not_false = self.negate(data.if_false);
        let if_true = self.and(not_true, not_uncertain);
        let if_false = self.and(not_false, not_uncertain);
        let result = NodeId::new(&mut self.storage, data.constraint, if_true, if_false);
        self.storage.negate_cache.insert(node, result);
        self.publications
            .push(StructuralState::capture(&self.storage));
        result
    }
}

fn run_with_publications(
    storage: &mut ConstraintSetStorage<'_>,
    operation: Operation,
) -> (NodeId, Vec<StructuralState>, usize) {
    let mut engine = TddApply::new(operation);
    let mut previous = StructuralState::capture(storage);
    let mut publications = Vec::new();
    for transitions in 1..=10_000 {
        let outcome = engine.advance(storage);
        let current = StructuralState::capture(storage);
        if current != previous {
            publications.push(current.clone());
            previous = current;
        }
        if let ControlFlow::Break(result) = outcome {
            return (result, publications, transitions);
        }
    }
    panic!("the finite fixture did not complete within 10,000 transitions");
}

fn mixed_operations(
    storage: &mut ConstraintSetStorage<'_>,
    ids: &[ConstraintId],
) -> Vec<Operation> {
    for constraint in ids {
        storage.constraint_source_order(*constraint);
    }
    let a = NodeId::new(storage, ids[0], ALWAYS_TRUE, ALWAYS_FALSE);
    let not_a = NodeId::new(storage, ids[0], ALWAYS_FALSE, ALWAYS_TRUE);
    let b = NodeId::new(storage, ids[1], ALWAYS_TRUE, ALWAYS_FALSE);
    let not_b = NodeId::new(storage, ids[1], ALWAYS_FALSE, ALWAYS_TRUE);
    let left = NodeId::with_uncertain(storage, ids[2], a, b, not_a);
    let right = NodeId::with_uncertain(storage, ids[2], not_b, not_a, b);
    vec![
        Operation::And(left, right),
        Operation::And(right, left),
        Operation::Or(left, right),
        Operation::Or(right, left),
        Operation::Negate(left),
        Operation::Negate(right),
        Operation::And(left, a),
        Operation::And(a, left),
        Operation::Or(left, a),
        Operation::Or(a, left),
        Operation::And(left, left),
        Operation::Or(left, left),
        Operation::And(ALWAYS_FALSE, left),
        Operation::And(left, ALWAYS_TRUE),
        Operation::Or(ALWAYS_TRUE, left),
        Operation::Or(left, ALWAYS_FALSE),
        Operation::Negate(ALWAYS_TRUE),
        Operation::Negate(ALWAYS_FALSE),
    ]
}

#[test]
fn mixed_operations_preserve_exact_interning_and_cache_publication_order() {
    let db = setup_db();
    let env = db.program_environment();
    let mut initial = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut initial, 3);
    let operations = mixed_operations(&mut initial, &ids);
    for operation in operations {
        let mut reference = RecursiveReference::new(&initial);
        let mut actual = fork_storage(&initial);
        let mut ordinary = fork_storage(&initial);
        for _ in 0..2 {
            reference.publications.clear();
            let expected = reference.apply(operation);
            let (result, publications, _) = run_with_publications(&mut actual, operation);
            assert_eq!(result, expected);
            assert_eq!(publications, reference.publications);
            assert_eq!(
                StructuralState::capture(&actual),
                StructuralState::capture(&reference.storage)
            );
            assert_eq!(operation.apply(&mut ordinary), expected);
            assert_eq!(
                StructuralState::capture(&ordinary),
                StructuralState::capture(&reference.storage)
            );
        }
    }
}

#[test]
fn dropping_at_each_transition_keeps_completed_work_reusable() {
    let db = setup_db();
    let env = db.program_environment();
    let mut initial = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut initial, 3);
    let operations = mixed_operations(&mut initial, &ids);
    let initial_state = StructuralState::capture(&initial);
    let mut retained_child = false;
    for operation in operations.into_iter().take(6) {
        let mut reference = RecursiveReference::new(&initial);
        let expected = reference.apply(operation);
        let expected_state = StructuralState::capture(&reference.storage);
        let mut complete = fork_storage(&initial);
        let (_, _, transitions) = run_with_publications(&mut complete, operation);
        for stop_after in 0..transitions {
            let mut storage = fork_storage(&initial);
            let mut interrupted = TddApply::new(operation);
            for _ in 0..stop_after {
                assert!(interrupted.advance(&mut storage).is_continue());
            }
            drop(interrupted);
            let partial = StructuralState::capture(&storage);
            assert!(
                partial == initial_state || reference.publications.contains(&partial),
                "dropping retains an exact prefix of completed publications",
            );
            let has_parent = match operation {
                Operation::And(left, right) => storage.and_cache.contains_key(&(left, right)),
                Operation::Or(left, right) => storage.or_cache.contains_key(&(left, right)),
                Operation::Negate(node) => storage.negate_cache.contains_key(&node),
            };
            retained_child |= !has_parent
                && (!storage.and_cache.is_empty()
                    || !storage.or_cache.is_empty()
                    || !storage.negate_cache.is_empty());
            let (retried, _, _) = run_with_publications(&mut storage, operation);
            assert_eq!(retried, expected);
            assert_eq!(StructuralState::capture(&storage), expected_state);
        }
    }
    assert!(
        retained_child,
        "the fixture must retain a completed child before its parent"
    );
}

#[test]
fn deep_unions_and_unique_node_collection_do_not_use_recursive_frames() -> std::io::Result<()> {
    let worker = thread::Builder::new().stack_size(128 * 1024).spawn(|| {
        let db = setup_db();
        let env = db.program_environment();
        let mut storage = ConstraintSetStorage::default();
        let constraints = constraints(&db, &env, &mut storage, 16_384);
        let mut all = ALWAYS_TRUE;
        let mut all_except_first = ALWAYS_TRUE;
        for (index, constraint) in constraints.iter().copied().enumerate() {
            all = NodeId::new(&mut storage, constraint, all, ALWAYS_FALSE);
            if index > 0 {
                all_except_first =
                    NodeId::new(&mut storage, constraint, all_except_first, ALWAYS_FALSE);
            }
        }
        assert_eq!(all.or(&mut storage, all_except_first), all_except_first);
        storage.or_cache.clear();
        assert_eq!(all_except_first.or(&mut storage, all), all_except_first);
        let mut visited = Vec::new();
        all.for_each_unique_constraint(&storage, &mut |constraint| {
            visited.push(constraint);
        });
        assert_eq!(visited, constraints.into_iter().rev().collect::<Vec<_>>());
    })?;
    assert!(
        worker.join().is_ok(),
        "complete small-stack union traversal"
    );
    Ok(())
}

#[test]
fn deep_conjunction_negation_and_mixed_operations_use_a_small_native_stack() -> std::io::Result<()>
{
    let worker = thread::Builder::new().stack_size(128 * 1024).spawn(|| {
        let db = setup_db();
        let env = db.program_environment();
        let mut storage = ConstraintSetStorage::default();
        let constraints = constraints(&db, &env, &mut storage, 16_384);
        let mut all = ALWAYS_TRUE;
        let mut left = ALWAYS_TRUE;
        let mut right = ALWAYS_TRUE;
        for (index, constraint) in constraints.iter().copied().enumerate() {
            all = NodeId::new(&mut storage, constraint, all, ALWAYS_FALSE);
            if index != 0 {
                left = NodeId::new(&mut storage, constraint, left, ALWAYS_FALSE);
            }
            if index != 1 {
                right = NodeId::new(&mut storage, constraint, right, ALWAYS_FALSE);
            }
        }
        assert_eq!(left.and(&mut storage, right), all);
        storage.and_cache.clear();
        assert_eq!(right.and(&mut storage, left), all);
        let complement = all.negate(&mut storage);
        assert_eq!(complement.negate(&mut storage), all);
        assert_eq!(all.and(&mut storage, complement), ALWAYS_FALSE);
    })?;
    assert!(
        worker.join().is_ok(),
        "complete small-stack mixed operations"
    );
    Ok(())
}

#[test]
fn unique_collection_preserves_order_without_revisiting_shared_children() {
    let db = setup_db();
    let env = db.program_environment();
    let mut storage = ConstraintSetStorage::default();
    let ids = constraints(&db, &env, &mut storage, 3);
    let leaf = NodeId::new(&mut storage, ids[0], ALWAYS_TRUE, ALWAYS_FALSE);
    let middle = NodeId::new(&mut storage, ids[1], leaf, ALWAYS_FALSE);
    // Intern directly to retain a deliberately shared graph regardless of local reductions.
    let root = storage.intern_interior_node(InteriorNodeData {
        constraint: ids[2],
        if_true: middle,
        if_uncertain: leaf,
        if_false: middle,
    });
    let mut visited = Vec::new();
    root.for_each_unique_constraint(&storage, &mut |constraint| {
        visited.push(constraint);
    });
    assert_eq!(visited, [ids[2], ids[1], ids[0]]);
}

// Run the same fixture against separately compiled operator implementations. Timed loops exclude
// database/fixture setup and result formatting. Cold loops include clearing the operator caches;
// node identities and their supports are already interned after the first repetition.
#[test]
#[ignore]
fn operator_cost_probe() -> std::io::Result<()> {
    let mut output = std::io::stdout().lock();
    let db = setup_db();
    let env = db.program_environment();
    for depth in [3, 16, 128] {
        let mut storage = ConstraintSetStorage::default();
        let ids = constraints(&db, &env, &mut storage, depth);
        let mut left = ALWAYS_TRUE;
        let mut right = ALWAYS_TRUE;
        for (index, constraint) in ids.iter().copied().enumerate() {
            if index != 0 {
                left = NodeId::new(&mut storage, constraint, left, ALWAYS_FALSE);
            }
            if index != 1 {
                right = NodeId::new(&mut storage, constraint, right, ALWAYS_FALSE);
            }
        }
        for operation in [
            Operation::And(left, right),
            Operation::Or(left, right),
            Operation::Negate(left),
        ] {
            let label = match operation {
                Operation::And(_, _) => "and",
                Operation::Or(_, _) => "or",
                Operation::Negate(_) => "negate",
            };
            for (warm, iterations) in [(false, 2_000), (true, 200_000)] {
                let start = std::time::Instant::now();
                for _ in 0..iterations {
                    if !warm {
                        storage.and_cache.clear();
                        storage.or_cache.clear();
                        storage.negate_cache.clear();
                    }
                    let result = match std::hint::black_box(operation) {
                        Operation::And(left, right) => left.and(&mut storage, right),
                        Operation::Or(left, right) => left.or(&mut storage, right),
                        Operation::Negate(node) => node.negate(&mut storage),
                    };
                    std::hint::black_box(result);
                }
                let elapsed = start.elapsed();
                writeln!(
                    output,
                    "operator={label} depth={depth} warm={warm} iterations={iterations} elapsed_ns={}",
                    elapsed.as_nanos()
                )?;
            }
        }
    }
    writeln!(
        output,
        "layout frame={} engine={}",
        size_of::<super::Frame>(),
        size_of::<TddApply>()
    )?;
    Ok(())
}

mod storage;
