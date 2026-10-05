use std::cell::RefCell;
use std::ops::ControlFlow;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use super::inputs;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintFold, ConstraintFoldKind, ConstraintId, ConstraintSet,
    ConstraintSetBuilder, ConstraintSetStorage, IteratorConstraintsExtension, NodeId, SourceOrder,
    SourceOrderId,
};

pub(super) type Value = (NodeId, Option<SourceOrderId>);
pub(super) type Entry = (NodeId, Option<SourceOrderId>, u8);

// Freeze the fold and sidecar algorithms independently of their production entry points. Graph
// operators and overlay readiness are shared dependencies with their own independent controls.
pub(super) struct ReferenceFold<'db> {
    pub(super) storage: ConstraintSetStorage<'db>,
    pub(super) kind: ConstraintFoldKind,
    pub(super) accumulator: SmallVec<[Entry; 8]>,
}

fn identity(kind: ConstraintFoldKind) -> NodeId {
    match kind {
        ConstraintFoldKind::All => ALWAYS_TRUE,
        ConstraintFoldKind::Any => ALWAYS_FALSE,
    }
}

fn absorbing(kind: ConstraintFoldKind) -> NodeId {
    match kind {
        ConstraintFoldKind::All => ALWAYS_FALSE,
        ConstraintFoldKind::Any => ALWAYS_TRUE,
    }
}

fn graph(
    kind: ConstraintFoldKind,
    storage: &mut ConstraintSetStorage<'_>,
    left: NodeId,
    right: NodeId,
) -> NodeId {
    match kind {
        ConstraintFoldKind::All => left.and(storage, right),
        ConstraintFoldKind::Any => left.or(storage, right),
    }
}

pub(super) fn intern_source(
    storage: &mut ConstraintSetStorage<'_>,
    data: SourceOrder,
) -> SourceOrderId {
    storage.ensure_overlay_identity_caches();
    if let Some(id) = storage.source_order_cache.get(&data) {
        return *id;
    }
    let local = storage.source_orders.push(data);
    let id = if let Some(compacted) = &storage.compacted {
        local + compacted.source_orders.len()
    } else {
        local
    };
    storage.source_order_cache.insert(data, id);
    id
}

pub(super) fn ordered_source(
    storage: &mut ConstraintSetStorage<'_>,
    left: Option<SourceOrderId>,
    right: Option<SourceOrderId>,
) -> Option<SourceOrderId> {
    match (left, right) {
        (None, None) => None,
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(left), Some(right)) => {
            Some(intern_source(storage, SourceOrder::Ordered(left, right)))
        }
    }
}

impl ReferenceFold<'_> {
    pub(super) fn push(&mut self, next: Value) -> ControlFlow<Value> {
        if next.0 == absorbing(self.kind) {
            return ControlFlow::Break(next);
        }
        let (mut node, mut source, mut depth) = (next.0, next.1, 0);
        while self
            .accumulator
            .last()
            .is_some_and(|entry| entry.2 == depth)
            && let Some((left, left_source, _)) = self.accumulator.pop()
        {
            node = graph(self.kind, &mut self.storage, left, node);
            source = ordered_source(&mut self.storage, left_source, source);
            if node == absorbing(self.kind) {
                return ControlFlow::Break((node, source));
            }
            depth += 1;
        }
        self.accumulator.push((node, source, depth));
        ControlFlow::Continue(())
    }

    pub(super) fn finish(&mut self) -> Value {
        let mut result = (identity(self.kind), None);
        for (node, source, _) in &self.accumulator {
            result.0 = graph(self.kind, &mut self.storage, result.0, *node);
            result.1 = ordered_source(&mut self.storage, result.1, *source);
        }
        result
    }
}

pub(super) fn fork_storage<'db>(storage: &ConstraintSetStorage<'db>) -> ConstraintSetStorage<'db> {
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
        constraint_bound_depth_cache: storage.constraint_bound_depth_cache.clone(),
        source_order_cache: storage.source_order_cache.clone(),
        never_satisfied_cache: storage.never_satisfied_cache.clone(),
        negate_cache: storage.negate_cache.clone(),
        or_cache: storage.or_cache.clone(),
        and_cache: storage.and_cache.clone(),
        exists_cache: storage.exists_cache.clone(),
    }
}

pub(super) fn assert_storage(
    actual: &ConstraintSetStorage<'_>,
    reference: &ConstraintSetStorage<'_>,
) {
    assert_eq!(actual.compacted, reference.compacted);
    assert_eq!(
        actual.overlay_identity_state,
        reference.overlay_identity_state
    );
    assert_eq!(actual.constraints.raw, reference.constraints.raw);
    assert_eq!(actual.typevars.raw, reference.typevars.raw);
    assert_eq!(actual.nodes.raw, reference.nodes.raw);
    assert_eq!(actual.supports.raw, reference.supports.raw);
    assert_eq!(
        actual.constraint_supports.raw,
        reference.constraint_supports.raw
    );
    assert_eq!(actual.node_supports.raw, reference.node_supports.raw);
    assert_eq!(actual.source_orders.raw, reference.source_orders.raw);
    assert_eq!(actual.constraint_cache, reference.constraint_cache);
    assert_eq!(actual.typevar_cache, reference.typevar_cache);
    assert_eq!(actual.node_cache, reference.node_cache);
    assert_eq!(
        actual.constraint_bound_depth_cache,
        reference.constraint_bound_depth_cache
    );
    assert_eq!(actual.source_order_cache, reference.source_order_cache);
    assert_eq!(
        actual.never_satisfied_cache,
        reference.never_satisfied_cache
    );
    assert_eq!(actual.negate_cache, reference.negate_cache);
    assert_eq!(actual.or_cache, reference.or_cache);
    assert_eq!(actual.and_cache, reference.and_cache);
    assert_eq!(actual.exists_cache, reference.exists_cache);
}

// This reader deliberately does not call calculate_source_orders: history order is part of the
// comparison, including histories attached to graphs that have become terminal.
pub(super) fn history(
    storage: &ConstraintSetStorage<'_>,
    source: Option<SourceOrderId>,
) -> Vec<ConstraintId> {
    let mut pending: Vec<_> = source.into_iter().collect();
    let mut visited = FxHashSet::default();
    let mut constraints = FxHashSet::default();
    let mut result = Vec::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        match storage.source_order_data(current) {
            SourceOrder::Ordered(left, right) => pending.extend([right, left]),
            SourceOrder::Constraint(constraint) => {
                if constraints.insert(constraint) {
                    result.push(constraint);
                }
            }
        }
    }
    result
}

pub(super) fn fixture(db: &TestDb) -> (ConstraintSetStorage<'_>, [Value; 9]) {
    let builder = ConstraintSetBuilder::new();
    let values = {
        let values = inputs(db, &builder);
        let not_a = values[0].negate(db, &builder);
        let mut storage = builder.storage.borrow_mut();
        let mut ordered = values;
        ordered.sort_by(|left, right| {
            storage
                .interior_node_data(left.node)
                .constraint
                .ordering()
                .cmp(&storage.interior_node_data(right.node).constraint.ordering())
        });
        let constraint = storage.interior_node_data(ordered[0].node).constraint;
        let uncertain = NodeId::with_uncertain(
            &mut storage,
            constraint,
            ordered[1].node,
            ordered[2].node,
            ordered[3].node,
        );
        [
            (values[0].node, values[0].source_order),
            (values[1].node, values[1].source_order),
            (values[2].node, values[2].source_order),
            (values[3].node, values[3].source_order),
            (values[4].node, values[4].source_order),
            (values[5].node, values[5].source_order),
            (values[6].node, values[6].source_order),
            (not_a.node, not_a.source_order),
            (uncertain, values[6].source_order),
        ]
    };
    (builder.storage.into_inner(), values)
}

fn compare(
    initial: &ConstraintSetStorage<'_>,
    kind: ConstraintFoldKind,
    values: &[Value],
) -> (Value, usize, Vec<ConstraintId>) {
    let builder = ConstraintSetBuilder {
        storage: RefCell::new(fork_storage(initial)),
    };
    let mut reference = ReferenceFold {
        storage: fork_storage(initial),
        kind,
        accumulator: SmallVec::new(),
    };
    let mut actual = ConstraintFold::new(&builder, kind);
    for (index, (node, source)) in values.iter().copied().enumerate() {
        let expected = reference.push((node, source));
        let result = actual
            .push(ConstraintSet::from_node(&builder, node, source))
            .map_break(|set| (set.node, set.source_order));
        assert_eq!(result, expected, "input {index}");
        assert_eq!(actual.accumulator, reference.accumulator, "input {index}");
        assert_storage(&builder.storage.borrow(), &reference.storage);
        if let ControlFlow::Break(result) = result {
            let expected_history = history(&reference.storage, result.1);
            assert_eq!(
                builder
                    .storage
                    .borrow()
                    .calculate_source_orders(result.1)
                    .into_iter()
                    .collect::<Vec<_>>(),
                expected_history
            );
            return (result, index + 1, expected_history);
        }
    }
    let expected = reference.finish();
    let result = actual.finish();
    assert_eq!((result.node, result.source_order), expected);
    assert_storage(&builder.storage.borrow(), &reference.storage);
    let expected_history = history(&reference.storage, expected.1);
    assert_eq!(
        builder
            .storage
            .borrow()
            .calculate_source_orders(result.source_order)
            .into_iter()
            .collect::<Vec<_>>(),
        expected_history
    );
    (expected, values.len(), expected_history)
}

#[test]
fn frozen_reference_matches_each_push_and_finalization() {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    let [a, b, c, d, _, _, _, not_a, uncertain] = values;
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for sequence in [
            vec![],
            vec![a],
            values[..7].to_vec(),
            vec![a, a, b, b, a],
            vec![(a.0, None), b, (c.0, None), d],
            vec![uncertain, b, d],
        ] {
            compare(&initial, kind, &sequence);
        }
        let terminal = (absorbing(kind), b.1);
        let (result, produced, _) = compare(&initial, kind, &[a, terminal, c]);
        assert_eq!(result, terminal);
        assert_eq!(produced, 2);
        let (result, produced, result_history) = compare(&initial, kind, &[b, c, a, not_a, d]);
        assert_eq!(result, (absorbing(kind), a.1));
        assert_eq!(produced, 4);
        assert_eq!(result_history, history(&initial, a.1));
    }
}

#[test]
fn frozen_reference_retains_late_history_after_finish_absorption() {
    let db = setup_db();
    let (initial, [a, b, _, d, _, _, _, not_a, _]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let (result, produced, result_history) = compare(
            &initial,
            kind,
            &[a, a, a, a, (not_a.0, b.1), (not_a.0, b.1), d],
        );
        assert_eq!(result.0, absorbing(kind));
        assert_eq!(produced, 7);
        let mut expected = history(&initial, a.1);
        expected.extend(history(&initial, b.1));
        expected.extend(history(&initial, d.1));
        assert_eq!(result_history, expected);
    }
}

#[test]
fn frozen_reference_matches_accumulator_spill_and_following_carry() {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = ConstraintSetBuilder {
            storage: RefCell::new(fork_storage(&initial)),
        };
        let mut reference = ReferenceFold {
            storage: fork_storage(&initial),
            kind,
            accumulator: SmallVec::new(),
        };
        let mut actual = ConstraintFold::new(&builder, kind);
        for index in 0..512 {
            let next = (values[0].0, values[index % 7].1);
            assert!(reference.push(next).is_continue());
            assert!(
                actual
                    .push(ConstraintSet::from_node(&builder, next.0, next.1))
                    .is_continue()
            );
            assert_eq!(actual.accumulator, reference.accumulator);
            assert_storage(&builder.storage.borrow(), &reference.storage);
            if index == 510 {
                assert_eq!(actual.accumulator.len(), 9);
                assert!(actual.accumulator.spilled());
            }
        }
        assert_eq!(actual.accumulator.len(), 1);
        let expected = reference.finish();
        let actual = actual.finish();
        assert_eq!((actual.node, actual.source_order), expected);
        assert_storage(&builder.storage.borrow(), &reference.storage);
    }
}

#[test]
fn frozen_source_interner_matches_leaf_ordered_and_overlay_identities() {
    let db = setup_db();
    let (mut initial, _) = fixture(&db);
    initial.source_orders.raw.clear();
    initial.source_order_cache.clear();
    let mut actual = fork_storage(&initial);
    let mut reference = fork_storage(&initial);
    let constraints: Vec<_> = initial
        .constraints
        .iter_enumerated()
        .map(|(id, _)| id)
        .collect();
    for constraint in constraints {
        assert_eq!(
            actual.constraint_source_order(constraint),
            intern_source(&mut reference, SourceOrder::Constraint(constraint))
        );
        assert_storage(&actual, &reference);
    }
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        inputs(&db, builder)
            .into_iter()
            .when_all(&db, builder, |value| value)
    });
    let overlay = ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    };
    for initial in [&actual, &overlay] {
        let mut actual = fork_storage(initial);
        let mut reference = fork_storage(initial);
        let first = SourceOrderId::from_usize(0);
        let second = SourceOrderId::from_usize(1);
        for (left, right) in [
            (None, None),
            (None, Some(first)),
            (Some(first), None),
            (Some(first), Some(first)),
            (Some(first), Some(second)),
            (Some(second), Some(first)),
            (Some(first), Some(second)),
        ] {
            assert_eq!(
                actual.ordered_source_order(left, right),
                ordered_source(&mut reference, left, right)
            );
            assert_storage(&actual, &reference);
        }
    }
}
