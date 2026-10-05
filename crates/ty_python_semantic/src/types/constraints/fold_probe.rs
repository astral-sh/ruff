use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::ops::ControlFlow;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
    IteratorConstraintsExtension, NodeId,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::{BoundTypeVarInstance, KnownClass, TypeVarVariance};

fn inputs<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
) -> [ConstraintSet<'db, 'c>; 7] {
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(db, &env);
    ["A", "B", "C", "D", "E", "F", "G"].map(|name| {
        let typevar = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, typevar, int)
    })
}

fn assert_same<'db, 'c>(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) {
    assert!(std::ptr::eq(left.builder, right.builder));
    assert_eq!(left.node, right.node);
    assert_eq!(left.source_order, right.source_order);
}

fn combine<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    mut left: ConstraintSet<'db, 'c>,
    right: ConstraintSet<'db, 'c>,
) -> ConstraintSet<'db, 'c> {
    match kind {
        ConstraintFoldKind::All => left.intersect(db, builder, right),
        ConstraintFoldKind::Any => left.union(db, builder, right),
    }
}

fn sync_fold<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    inputs: &[ConstraintSet<'db, 'c>],
    produced: &Cell<usize>,
) -> ConstraintSet<'db, 'c> {
    let produce = |input| {
        produced.set(produced.get() + 1);
        input
    };
    match kind {
        ConstraintFoldKind::All => inputs.iter().copied().when_all(db, builder, produce),
        ConstraintFoldKind::Any => inputs.iter().copied().when_any(db, builder, produce),
    }
}

// Each set bit suspends before producing that input. The fold itself retains ordinary locals;
// no builder borrow may survive while an input's evaluation is pending.
async fn suspended_fold<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    inputs: &[ConstraintSet<'db, 'c>],
    pauses: usize,
    produced: &Cell<usize>,
) -> ConstraintSet<'db, 'c> {
    let mut fold = ConstraintFold::new(builder, kind);
    for (index, input) in inputs.iter().copied().enumerate() {
        let mut pause = pauses & (1 << index) != 0;
        poll_fn(|context| {
            if std::mem::take(&mut pause) {
                context.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        produced.set(produced.get() + 1);
        if let ControlFlow::Break(result) = fold.push(input) {
            return result;
        }
    }
    fold.finish()
}

#[test]
fn empty_inputs_return_the_identity() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let expected = ConstraintSet::from_node(&builder, kind.identity(), None);
        assert_same(ConstraintFold::new(&builder, kind).finish(), expected);
        let produced = Cell::new(0);
        assert_same(sync_fold(&db, &builder, kind, &[], &produced), expected);
        assert_eq!(produced.get(), 0);
    }
}

#[test]
fn terminal_input_preserves_its_own_history_and_stops_production() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let [a, b, c, ..] = inputs(&db, &builder);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let terminal = ConstraintSet::from_node(&builder, kind.absorbing(), b.source_order);
        let produced = Cell::new(0);
        let result = sync_fold(&db, &builder, kind, &[a, terminal, c], &produced);
        assert_same(result, terminal);
        assert_eq!(produced.get(), 2);
    }
}

#[test]
fn intermediate_saturation_stops_before_the_next_effect() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let [a, b, c, d, ..] = inputs(&db, &builder);
    let not_c = c.negate(&db, &builder);
    assert!(!c.node.is_terminal());
    assert!(!not_c.node.is_terminal());

    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let expected = combine(&db, &builder, kind, c, not_c);
        assert_eq!(expected.node, kind.absorbing());
        let produced = Cell::new(0);
        let result = sync_fold(&db, &builder, kind, &[a, b, c, not_c, d], &produced);
        // The earlier A/B subtree is not included in the history of a saturating C/not-C pair.
        assert_same(result, expected);
        assert_eq!(produced.get(), 4);

        let mut fold = ConstraintFold::new(&builder, kind);
        for input in [a, b, c] {
            assert!(fold.push(input).is_continue());
        }
        let ControlFlow::Break(result) = fold.push(not_c) else {
            panic!("complementary constraints must saturate their pair");
        };
        assert_same(result, expected);
    }
}

#[test]
fn raw_node_entrypoints_retain_balanced_source_order() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let values @ [a, b, c, d, e, f, g] = inputs(&db, &builder);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let pair = |left, right| combine(&db, &builder, kind, left, right);
        let expected = pair(pair(pair(pair(a, b), pair(c, d)), pair(e, f)), g);
        let raw = values.iter().map(|set| (set.node, set.source_order));
        let (node, source_order) = match kind {
            ConstraintFoldKind::All => NodeId::distributed_and(&builder, raw),
            ConstraintFoldKind::Any => NodeId::distributed_or(&builder, raw),
        };
        assert_same(
            ConstraintSet::from_node(&builder, node, source_order),
            expected,
        );
        let produced = Cell::new(0);
        assert_same(sync_fold(&db, &builder, kind, &values, &produced), expected);
        assert_eq!(produced.get(), values.len());
        assert!(!expected.node.is_terminal());
    }
}

#[test]
fn suspension_boundaries_preserve_results_and_early_termination() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let values = inputs(&db, &builder);
    let [a, b, c, d, ..] = values;
    let saturating = [a, b, c, c.negate(&db, &builder), d];
    let mut context = Context::from_waker(Waker::noop());

    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for values in [values.as_slice(), saturating.as_slice()] {
            let expected_produced = Cell::new(0);
            let expected = sync_fold(&db, &builder, kind, values, &expected_produced);
            // Every chunking, including each chunking's complement, uses the same input order.
            for pauses in 0..(1 << values.len()) {
                let produced = Cell::new(0);
                let mut future = pin!(suspended_fold(&builder, kind, values, pauses, &produced));
                let mut pending = 0;
                loop {
                    match future.as_mut().poll(&mut context) {
                        Poll::Ready(result) => {
                            assert_same(result, expected);
                            assert_eq!(produced.get(), expected_produced.get());
                            let consumed = (1 << produced.get()) - 1;
                            assert_eq!(pending, (pauses & consumed).count_ones());
                            break;
                        }
                        Poll::Pending => {
                            pending += 1;
                            assert!(
                                usize::try_from(pending)
                                    .is_ok_and(|pending| pending <= values.len())
                            );
                            assert!(builder.storage.try_borrow_mut().is_ok());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn dropping_a_suspended_fold_releases_its_local_state() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let values = inputs(&db, &builder);
    let produced = Cell::new(0);
    let mut context = Context::from_waker(Waker::noop());
    {
        let mut future = pin!(suspended_fold(
            &builder,
            ConstraintFoldKind::All,
            &values,
            1 << 3,
            &produced,
        ));
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(produced.get(), 3);
        assert!(builder.storage.try_borrow_mut().is_ok());
    }
    assert!(builder.storage.try_borrow_mut().is_ok());
    produced.set(0);
    let result = sync_fold(&db, &builder, ConstraintFoldKind::All, &values, &produced);
    assert!(!result.node.is_terminal());
    assert_eq!(produced.get(), values.len());
}

mod controlled;
mod cost_probe;
mod prepared;
mod reference;
