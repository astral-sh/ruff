use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::ops::ControlFlow;
use std::task::{Context, Poll, Waker};

use smallvec::SmallVec;

use super::inputs;
use super::reference::{
    Entry, ReferenceFold, Value, assert_storage, fixture, fork_storage, intern_source,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::apply::{Operation, TddApply};
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork,
};
use crate::types::constraints::source_order::{PendingSourceOrder, next_source_order_id};
use crate::types::constraints::{
    ConstraintCombination, ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
    ConstraintSetStorage, IteratorConstraintsExtension, SourceOrder, SourceOrderId,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::relation::execution::attempt::AttemptAdmission;

mod trace;

#[derive(Default)]
struct RecordingControl {
    events: Vec<TddWork>,
    refuse: Option<usize>,
}

impl TddControl for RecordingControl {
    type Error = usize;

    fn admit(&mut self, work: TddWork) -> Result<(), usize> {
        let index = self.events.len();
        self.events.push(work);
        if self.refuse == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}

fn reference<'db>(
    initial: &ConstraintSetStorage<'db>,
    kind: ConstraintFoldKind,
) -> ReferenceFold<'db> {
    ReferenceFold {
        storage: fork_storage(initial),
        kind,
        accumulator: SmallVec::new(),
    }
}

fn builder<'db>(initial: &ConstraintSetStorage<'db>) -> ConstraintSetBuilder<'db> {
    ConstraintSetBuilder {
        storage: RefCell::new(fork_storage(initial)),
    }
}

fn fold<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    accumulator: &SmallVec<[Entry; 8]>,
) -> ConstraintFold<'db, 'c> {
    ConstraintFold {
        builder,
        kind,
        accumulator: accumulator.clone(),
    }
}

fn check_publication_boundary(events: &[TddWork]) {
    let graph = events
        .iter()
        .filter(|work| matches!(work, TddWork::Commit))
        .count();
    let sidecar = events
        .iter()
        .filter(|work| matches!(work, TddWork::SourceOrderCommit))
        .count();
    assert!(
        graph + sidecar <= 1,
        "one advance publishes at most one child"
    );
}

fn push(
    fold: &mut ConstraintFold<'_, '_>,
    next: Value,
    control: &mut RecordingControl,
) -> Result<(ControlFlow<Value>, usize), TddError<usize>> {
    let builder = fold.builder;
    let mut cursor = fold.begin_push(ConstraintSet::from_node(builder, next.0, next.1));
    for transitions in 1..100_000 {
        let start = control.events.len();
        let progress = cursor.advance_with(control);
        assert!(builder.storage.try_borrow_mut().is_ok());
        check_publication_boundary(&control.events[start..]);
        if let ControlFlow::Break(result) = progress? {
            return Ok((
                result.map_break(|set| (set.node, set.source_order)),
                transitions,
            ));
        }
    }
    panic!("the finite push fixture did not complete");
}

fn finish(
    fold: &mut ConstraintFold<'_, '_>,
    control: &mut RecordingControl,
) -> Result<(Value, usize), TddError<usize>> {
    let builder = fold.builder;
    let mut cursor = fold.begin_finish();
    for transitions in 1..100_000 {
        let start = control.events.len();
        let progress = cursor.advance_with(control);
        assert!(builder.storage.try_borrow_mut().is_ok());
        check_publication_boundary(&control.events[start..]);
        if let ControlFlow::Break(result) = progress? {
            return Ok(((result.node, result.source_order), transitions));
        }
    }
    panic!("the finite finish fixture did not complete");
}

fn source(
    storage: &mut ConstraintSetStorage<'_>,
    data: SourceOrder,
    control: &mut RecordingControl,
) -> Result<SourceOrderId, TddError<usize>> {
    let mut cursor = PendingSourceOrder::new(data);
    for _ in 0..100_000 {
        let start = control.events.len();
        let progress = cursor.advance_with(storage, control);
        check_publication_boundary(&control.events[start..]);
        if let ControlFlow::Break(result) = progress? {
            return Ok(result);
        }
    }
    panic!("the finite source-order fixture did not complete");
}

fn combination(
    builder: &ConstraintSetBuilder<'_>,
    kind: ConstraintFoldKind,
    left: Value,
    right: Value,
    control: &mut RecordingControl,
) -> Result<(Value, usize), TddError<usize>> {
    let mut cursor = ConstraintCombination::new(
        builder,
        kind,
        ConstraintSet::from_node(builder, left.0, left.1),
        ConstraintSet::from_node(builder, right.0, right.1),
    );
    for transitions in 1..100_000 {
        let start = control.events.len();
        let progress = cursor.advance_with(control);
        assert!(builder.storage.try_borrow_mut().is_ok());
        check_publication_boundary(&control.events[start..]);
        if let ControlFlow::Break(result) = progress? {
            return Ok(((result.node, result.source_order), transitions));
        }
    }
    panic!("the finite combination fixture did not complete");
}

fn ordinary_combination<'db>(
    db: &'db TestDb,
    builder: &ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    left: Value,
    right: Value,
) -> Value {
    let mut left = ConstraintSet::from_node(builder, left.0, left.1);
    let right = ConstraintSet::from_node(builder, right.0, right.1);
    let result = match kind {
        ConstraintFoldKind::All => left.intersect(db, builder, right),
        ConstraintFoldKind::Any => left.union(db, builder, right),
    };
    (result.node, result.source_order)
}

#[test]
fn direct_combination_refusal_and_drop_allow_ordinary_retry() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, [left, right, ..]) = fixture(&db);
    let mut graph_without_sidecar = false;
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let mut expected = reference(&initial, kind);
        assert!(expected.push(left).is_continue());
        assert!(expected.push(right).is_continue());
        let expected_result = expected.finish();
        let complete_builder = builder(&initial);
        let mut trace = RecordingControl::default();
        let (actual, transitions) = combination(&complete_builder, kind, left, right, &mut trace)?;
        assert_eq!(actual, expected_result);
        assert_storage(&complete_builder.storage.borrow(), &expected.storage);
        for refusal in 0..trace.events.len() {
            let builder = builder(&initial);
            let mut control = RecordingControl {
                refuse: Some(refusal),
                ..RecordingControl::default()
            };
            assert_eq!(
                combination(&builder, kind, left, right, &mut control),
                Err(TddError::Refused(refusal)),
            );
            assert_eq!(control.events.len(), refusal + 1);
            {
                let storage = builder.storage.borrow();
                let graph_complete = match kind {
                    ConstraintFoldKind::All => storage.and_cache.contains_key(&(left.0, right.0)),
                    ConstraintFoldKind::Any => storage.or_cache.contains_key(&(left.0, right.0)),
                };
                graph_without_sidecar |= graph_complete;
                assert_eq!(storage.source_orders.raw, initial.source_orders.raw);
            }
            assert_eq!(
                ordinary_combination(&db, &builder, kind, left, right),
                expected_result,
            );
            assert_storage(&builder.storage.borrow(), &expected.storage);
        }
        for stop_after in 0..transitions {
            let builder = builder(&initial);
            {
                let mut cursor = ConstraintCombination::new(
                    &builder,
                    kind,
                    ConstraintSet::from_node(&builder, left.0, left.1),
                    ConstraintSet::from_node(&builder, right.0, right.1),
                );
                let mut control = RecordingControl::default();
                for _ in 0..stop_after {
                    let start = control.events.len();
                    assert!(cursor.advance_with(&mut control)?.is_continue());
                    assert!(builder.storage.try_borrow_mut().is_ok());
                    check_publication_boundary(&control.events[start..]);
                }
            }
            assert!(builder.storage.try_borrow_mut().is_ok());
            assert_eq!(
                ordinary_combination(&db, &builder, kind, left, right),
                expected_result,
            );
            assert_storage(&builder.storage.borrow(), &expected.storage);
        }
    }
    assert!(
        graph_without_sidecar,
        "graph completion precedes sidecar admission and result delivery"
    );
    Ok(())
}

#[test]
fn combination_preserves_the_exact_child_operator_admission_stream() -> Result<(), TddError<usize>>
{
    let db = setup_db();
    let (initial, [a, b, _, _, _, _, _, _, uncertain]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for (left, right) in [(a, b), (uncertain, a)] {
            let operation = match kind {
                ConstraintFoldKind::All => Operation::And(left.0, right.0),
                ConstraintFoldKind::Any => Operation::Or(left.0, right.0),
            };
            for warm in [false, true] {
                let mut initial = fork_storage(&initial);
                if warm {
                    operation.apply(&mut initial);
                }
                let mut expected_storage = fork_storage(&initial);
                let mut expected = RecordingControl::default();
                let mut operator = TddApply::new(operation);
                let mut expected_node = None;
                for _ in 0..100_000 {
                    if let ControlFlow::Break(node) =
                        operator.advance_with(&mut expected_storage, &mut expected)?
                    {
                        expected_node = Some(node);
                        break;
                    }
                }
                let builder = builder(&initial);
                let mut actual = RecordingControl::default();
                let (result, _) = combination(&builder, kind, left, right, &mut actual)?;
                assert_eq!(Some(result.0), expected_node);
                assert!(actual.events.contains(&TddWork::SourceOrderAdvance));
                let operator_events: Vec<_> = actual
                    .events
                    .iter()
                    .copied()
                    .take_while(|event| *event != TddWork::SourceOrderAdvance)
                    .filter(|event| *event != TddWork::CombinationAdvance)
                    .collect();
                assert_eq!(operator_events, expected.events);
            }
        }
    }
    Ok(())
}

fn push_refusals(
    initial: &ConstraintSetStorage<'_>,
    kind: ConstraintFoldKind,
    prefix: &[Value],
    next: Value,
) -> Result<Vec<TddWork>, TddError<usize>> {
    let mut expected = reference(initial, kind);
    for value in prefix {
        assert!(expected.push(*value).is_continue());
    }
    let before_storage = fork_storage(&expected.storage);
    let before_accumulator = expected.accumulator.clone();
    let result = expected.push(next);
    let complete_builder = builder(&before_storage);
    let mut complete = fold(&complete_builder, kind, &before_accumulator);
    let mut trace = RecordingControl::default();
    assert_eq!(push(&mut complete, next, &mut trace)?.0, result);
    assert_eq!(complete.accumulator, expected.accumulator);
    assert_storage(&complete_builder.storage.borrow(), &expected.storage);
    for refusal in 0..trace.events.len() {
        let builder = builder(&before_storage);
        let mut actual = fold(&builder, kind, &before_accumulator);
        let mut control = RecordingControl {
            refuse: Some(refusal),
            ..RecordingControl::default()
        };
        assert_eq!(
            push(&mut actual, next, &mut control),
            Err(TddError::Refused(refusal)),
        );
        assert_eq!(control.events.len(), refusal + 1);
        assert_eq!(actual.accumulator, before_accumulator);
        assert!(builder.storage.try_borrow_mut().is_ok());
        assert_eq!(
            push(&mut actual, next, &mut RecordingControl::default())?.0,
            result
        );
        assert_eq!(actual.accumulator, expected.accumulator);
        assert_storage(&builder.storage.borrow(), &expected.storage);
    }
    Ok(trace.events)
}

#[test]
fn each_push_admission_preserves_accepted_entries_on_refusal() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, [a, b, c, d, _, _, _, not_a, uncertain]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for (prefix, next) in [
            (vec![], a),
            (vec![a], b),
            (vec![a, a, a], b),
            (vec![b, c, a], not_a),
            (vec![uncertain, b, c], d),
            (vec![a], (kind.absorbing(), b.1)),
        ] {
            push_refusals(&initial, kind, &prefix, next)?;
        }
    }
    Ok(())
}

#[test]
fn each_finish_admission_can_retry_with_all_later_history() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    let [a, b, _, d, _, _, _, not_a, _] = values;
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for prefix in [
            vec![],
            vec![a],
            values[..7].to_vec(),
            vec![a, a, a, a, (not_a.0, b.1), (not_a.0, b.1), d],
        ] {
            let mut expected = reference(&initial, kind);
            for value in prefix {
                assert!(expected.push(value).is_continue());
            }
            let before_storage = fork_storage(&expected.storage);
            let before_accumulator = expected.accumulator.clone();
            let result = expected.finish();
            let complete_builder = builder(&before_storage);
            let mut complete = fold(&complete_builder, kind, &before_accumulator);
            let mut trace = RecordingControl::default();
            let (actual, transitions) = finish(&mut complete, &mut trace)?;
            assert_eq!(actual, result);
            assert_storage(&complete_builder.storage.borrow(), &expected.storage);
            for refusal in 0..trace.events.len() {
                let builder = builder(&before_storage);
                let mut actual = fold(&builder, kind, &before_accumulator);
                let mut control = RecordingControl {
                    refuse: Some(refusal),
                    ..RecordingControl::default()
                };
                assert_eq!(
                    finish(&mut actual, &mut control),
                    Err(TddError::Refused(refusal))
                );
                assert_eq!(control.events.len(), refusal + 1);
                assert_eq!(actual.accumulator, before_accumulator);
                assert!(builder.storage.try_borrow_mut().is_ok());
                assert_eq!(
                    finish(&mut actual, &mut RecordingControl::default())?.0,
                    result
                );
                assert_storage(&builder.storage.borrow(), &expected.storage);
            }
            for stop_after in 0..transitions {
                let builder = builder(&before_storage);
                let mut actual = fold(&builder, kind, &before_accumulator);
                {
                    let mut cursor = actual.begin_finish();
                    for _ in 0..stop_after {
                        assert!(
                            cursor
                                .advance_with(&mut RecordingControl::default())?
                                .is_continue()
                        );
                        assert!(builder.storage.try_borrow_mut().is_ok());
                    }
                }
                assert_eq!(actual.accumulator, before_accumulator);
                assert_eq!(
                    finish(&mut actual, &mut RecordingControl::default())?.0,
                    result
                );
                assert_storage(&builder.storage.borrow(), &expected.storage);
            }
        }
    }
    Ok(())
}

#[test]
fn dropping_each_push_transition_keeps_the_input_unaccepted() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, [a, b, c, d, _, _, _, not_a, _]) = fixture(&db);
    let mut retained_sidecar = false;
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for (prefix, next) in [(vec![a, b, c], d), (vec![b, c, a], not_a)] {
            let mut expected = reference(&initial, kind);
            for value in prefix {
                assert!(expected.push(value).is_continue());
            }
            let before_storage = fork_storage(&expected.storage);
            let before_accumulator = expected.accumulator.clone();
            let result = expected.push(next);
            let complete_builder = builder(&before_storage);
            let mut complete = fold(&complete_builder, kind, &before_accumulator);
            let (_, transitions) = push(&mut complete, next, &mut RecordingControl::default())?;
            for stop_after in 0..transitions {
                let builder = builder(&before_storage);
                let mut actual = fold(&builder, kind, &before_accumulator);
                {
                    let mut cursor =
                        actual.begin_push(ConstraintSet::from_node(&builder, next.0, next.1));
                    let mut control = RecordingControl::default();
                    for _ in 0..stop_after {
                        assert!(cursor.advance_with(&mut control)?.is_continue());
                        assert!(builder.storage.try_borrow_mut().is_ok());
                    }
                }
                assert_eq!(actual.accumulator, before_accumulator);
                retained_sidecar |= builder.storage.borrow().source_orders.len()
                    > before_storage.source_orders.len();
                assert_eq!(
                    push(&mut actual, next, &mut RecordingControl::default())?.0,
                    result
                );
                assert_eq!(actual.accumulator, expected.accumulator);
                assert_storage(&builder.storage.borrow(), &expected.storage);
            }
        }
    }
    assert!(
        retained_sidecar,
        "a completed sidecar survives before fold acceptance"
    );
    Ok(())
}

#[test]
fn accumulator_spill_is_admitted_before_growth_or_acceptance() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    let prefix: Vec<_> = (0..510)
        .map(|index| (values[0].0, values[index % 7].1))
        .collect();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let trace = push_refusals(&initial, kind, &prefix, (values[0].0, values[6].1))?;
        assert!(trace.iter().any(|work| matches!(
            work,
            TddWork::Grow {
                allocation: AllocationKind::FoldAccumulator,
                ..
            }
        )));
    }
    Ok(())
}

#[test]
fn source_admissions_preserve_atomic_identity_and_allow_ordinary_retry()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, _) = fixture(&db);
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        inputs(&db, builder)
            .into_iter()
            .when_all(&db, builder, |value| value)
    });
    let overlay = ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    };
    let data = SourceOrder::Ordered(SourceOrderId::from_usize(1), SourceOrderId::from_usize(0));
    for initial in [&initial, &overlay] {
        for warm in [false, true] {
            let mut initial = fork_storage(initial);
            if warm {
                intern_source(&mut initial, data);
            }
            let mut expected = fork_storage(&initial);
            let result = intern_source(&mut expected, data);
            let mut complete = fork_storage(&initial);
            let mut trace = RecordingControl::default();
            assert_eq!(source(&mut complete, data, &mut trace)?, result);
            assert_storage(&complete, &expected);
            for refusal in 0..trace.events.len() {
                let mut actual = fork_storage(&initial);
                let mut control = RecordingControl {
                    refuse: Some(refusal),
                    ..RecordingControl::default()
                };
                assert_eq!(
                    source(&mut actual, data, &mut control),
                    Err(TddError::Refused(refusal))
                );
                assert_eq!(control.events.len(), refusal + 1);
                assert_eq!(actual.source_orders.raw, initial.source_orders.raw);
                assert_eq!(actual.intern_source_order(data), result);
                assert_storage(&actual, &expected);
            }
        }
    }
    Ok(())
}

#[test]
fn interleaved_source_cursors_reuse_one_overlay_identity() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let owned = ConstraintSetBuilder::new().into_owned(|builder| {
        inputs(&db, builder)
            .into_iter()
            .when_all(&db, builder, |value| value)
    });
    let mut storage = ConstraintSetStorage {
        compacted: owned.inner.clone(),
        ..ConstraintSetStorage::default()
    };
    let data = SourceOrder::Ordered(SourceOrderId::from_usize(1), SourceOrderId::from_usize(0));
    let mut expected = fork_storage(&storage);
    let result = intern_source(&mut expected, data);
    let mut cursors = [PendingSourceOrder::new(data), PendingSourceOrder::new(data)];
    let mut results = [None, None];
    let mut control = RecordingControl::default();
    for _ in 0..100_000 {
        for (cursor, result) in cursors.iter_mut().zip(&mut results) {
            if result.is_none()
                && let ControlFlow::Break(id) = cursor.advance_with(&mut storage, &mut control)?
            {
                *result = Some(id);
            }
        }
        if results.iter().all(Option::is_some) {
            assert_eq!(results, [Some(result), Some(result)]);
            assert_storage(&storage, &expected);
            assert_eq!(
                control
                    .events
                    .iter()
                    .filter(|work| matches!(work, TddWork::SourceOrderCommit))
                    .count(),
                1
            );
            return Ok(());
        }
    }
    panic!("the finite interleaving fixture did not complete");
}

#[test]
fn source_order_growth_refuses_before_publication_and_keeps_warm_progress()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let (mut storage, _) = fixture(&db);
    let seed = SourceOrderId::from_usize(0);
    let mut tail = SourceOrderId::from_usize(storage.source_orders.len() - 1);
    let mut growths = 0;
    for index in 0..96 {
        let data = SourceOrder::Ordered(tail, seed);
        let initial = fork_storage(&storage);
        let mut expected = fork_storage(&initial);
        let expected_id = intern_source(&mut expected, data);
        let mut trace = RecordingControl::default();
        let actual = source(&mut storage, data, &mut trace)?;
        assert_eq!(actual, expected_id);
        assert_storage(&storage, &expected);
        if trace.events.iter().any(|work| {
            matches!(
                work,
                TddWork::Grow {
                    allocation: AllocationKind::Table(TableKind::SourceOrders),
                    ..
                }
            )
        }) {
            growths += 1;
            for refusal in 0..trace.events.len() {
                let mut partial = fork_storage(&initial);
                let mut control = RecordingControl {
                    refuse: Some(refusal),
                    ..RecordingControl::default()
                };
                assert_eq!(
                    source(&mut partial, data, &mut control),
                    Err(TddError::Refused(refusal))
                );
                assert_eq!(partial.source_orders.raw, initial.source_orders.raw);
                assert_eq!(partial.source_order_cache, initial.source_order_cache);
                assert_eq!(
                    source(&mut partial, data, &mut RecordingControl::default())?,
                    expected_id
                );
                assert_storage(&partial, &expected);
            }
        }
        let mut warm = RecordingControl::default();
        assert_eq!(source(&mut storage, data, &mut warm)?, expected_id);
        assert_eq!(warm.events.len(), 2);
        assert_eq!(warm.events[0], TddWork::SourceOrderAdvance);
        assert!(matches!(
            warm.events[1],
            TddWork::HashAccess {
                table: TableKind::SourceOrders,
                ..
            }
        ));
        assert_eq!(
            warm.events
                .iter()
                .map(|work| work.work_units())
                .sum::<usize>(),
            2
        );
        if [0, 95].contains(&index) {
            let mut completed = 0;
            let (limited, _) = expansion_probe::run(&db, 6, || {
                let admission = AttemptAdmission { db: &db };
                let mut control = ExecutionControl::new(&admission);
                for _ in 0..8 {
                    let mut cursor = PendingSourceOrder::new(data);
                    loop {
                        if let ControlFlow::Break(id) =
                            cursor.advance_with(&mut storage, &mut control)?
                        {
                            assert_eq!(id, expected_id);
                            completed += 1;
                            break;
                        }
                    }
                }
                Ok::<_, TddError<Incomplete>>(())
            });
            assert_eq!(limited, Err(Incomplete::Allowance));
            assert_eq!(completed, 3);
            assert_eq!(
                source(&mut storage, data, &mut RecordingControl::default())?,
                expected_id
            );
        }
        assert_storage(&storage, &expected);
        tail = actual;
    }
    assert!(growths >= 2);
    Ok(())
}

#[test]
fn source_id_exhaustion_is_detected_without_large_arenas() {
    assert_eq!(
        next_source_order_id::<usize>(1, 2),
        Ok(SourceOrderId::from_usize(3))
    );
    let maximum = (u32::MAX - 1) as usize;
    assert_eq!(
        next_source_order_id::<usize>(maximum, 0),
        Ok(SourceOrderId::from_usize(maximum))
    );
    for (local, overlay) in [
        (usize::MAX, 1),
        (1, usize::MAX),
        (maximum, 1),
        (0, u32::MAX as usize),
    ] {
        assert_eq!(
            next_source_order_id::<usize>(local, overlay),
            Err(TddError::CapacityExhausted)
        );
    }
}

#[test]
fn depth_exhaustion_leaves_all_accepted_entries_unchanged() {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    let a = values[0];
    // An artificial accumulator reaches the numeric boundary without producing 2^256 inputs.
    let accepted: SmallVec<[Entry; 8]> =
        (0..=u8::MAX).rev().map(|depth| (a.0, a.1, depth)).collect();
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let builder = builder(&initial);
        let mut actual = fold(&builder, kind, &accepted);
        assert_eq!(
            push(&mut actual, a, &mut RecordingControl::default()),
            Err(TddError::CapacityExhausted),
        );
        assert_eq!(actual.accumulator, accepted);
        assert!(builder.storage.try_borrow_mut().is_ok());
    }
}

async fn fold_root<'db, 'c>(
    builder: &'c ConstraintSetBuilder<'db>,
    kind: ConstraintFoldKind,
    values: &[Value],
    produced: &Cell<usize>,
) -> Result<ConstraintSet<'db, 'c>, TddError<usize>> {
    let mut fold = ConstraintFold::new(builder, kind);
    let mut control = RecordingControl::default();
    for (node, source) in values {
        produced.set(produced.get() + 1);
        let mut cursor = fold.begin_push(ConstraintSet::from_node(builder, *node, *source));
        let result = poll_fn(|context| match cursor.advance_with(&mut control) {
            Ok(ControlFlow::Continue(())) => {
                context.waker().wake_by_ref();
                Poll::Pending
            }
            Ok(ControlFlow::Break(result)) => Poll::Ready(Ok(result)),
            Err(error) => Poll::Ready(Err(error)),
        })
        .await?;
        if let ControlFlow::Break(result) = result {
            return Ok(result);
        }
    }
    let mut cursor = fold.begin_finish();
    poll_fn(|context| match cursor.advance_with(&mut control) {
        Ok(ControlFlow::Continue(())) => {
            context.waker().wake_by_ref();
            Poll::Pending
        }
        Ok(ControlFlow::Break(result)) => Poll::Ready(Ok(result)),
        Err(error) => Poll::Ready(Err(error)),
    })
    .await
}

#[test]
fn dropping_each_root_poll_releases_storage_and_stops_input_production()
-> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, values) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        let mut expected = reference(&initial, kind);
        for value in &values[..7] {
            assert!(expected.push(*value).is_continue());
        }
        let expected_result = expected.finish();
        let complete_builder = builder(&initial);
        let produced = Cell::new(0);
        let mut context = Context::from_waker(Waker::noop());
        let mut complete = Box::pin(fold_root(&complete_builder, kind, &values[..7], &produced));
        let mut polls = 0;
        loop {
            polls += 1;
            assert!(polls < 100_000);
            if let Poll::Ready(result) = complete.as_mut().poll(&mut context) {
                let result = result?;
                assert_eq!((result.node, result.source_order), expected_result);
                break;
            }
            assert!(complete_builder.storage.try_borrow_mut().is_ok());
        }
        assert_eq!(produced.get(), 7);
        for stop_after in 0..polls {
            let builder = builder(&initial);
            let produced = Cell::new(0);
            {
                let mut future = Box::pin(fold_root(&builder, kind, &values[..7], &produced));
                for _ in 0..stop_after {
                    assert!(future.as_mut().poll(&mut context).is_pending());
                    assert!(builder.storage.try_borrow_mut().is_ok());
                }
            }
            let before_retry = produced.get();
            assert!(builder.storage.try_borrow_mut().is_ok());
            assert_eq!(produced.get(), before_retry);
            produced.set(0);
            let mut retry = Box::pin(fold_root(&builder, kind, &values[..7], &produced));
            let mut retry_polls = 0;
            loop {
                retry_polls += 1;
                assert!(retry_polls < 100_000);
                if let Poll::Ready(result) = retry.as_mut().poll(&mut context) {
                    let result = result?;
                    assert_eq!((result.node, result.source_order), expected_result);
                    break;
                }
                assert!(builder.storage.try_borrow_mut().is_ok());
            }
            assert_eq!(produced.get(), 7);
            assert_storage(&builder.storage.borrow(), &expected.storage);
        }
    }
    Ok(())
}

#[test]
fn controlled_absorption_stops_before_producing_the_next_input() -> Result<(), TddError<usize>> {
    let db = setup_db();
    let (initial, [a, b, c, d, _, _, _, not_a, _]) = fixture(&db);
    for kind in [ConstraintFoldKind::All, ConstraintFoldKind::Any] {
        for (values, consumed) in [
            (vec![a, (kind.absorbing(), b.1), d], 2),
            (vec![b, c, a, not_a, d], 4),
        ] {
            let mut expected = reference(&initial, kind);
            let mut result = ControlFlow::Continue(());
            for value in values.iter().take(consumed) {
                result = expected.push(*value);
            }
            assert!(result.is_break());
            let builder = builder(&initial);
            let produced = Cell::new(0);
            let mut context = Context::from_waker(Waker::noop());
            let mut future = Box::pin(fold_root(&builder, kind, &values, &produced));
            let mut polls = 0;
            loop {
                polls += 1;
                assert!(polls < 100_000);
                if let Poll::Ready(actual) = future.as_mut().poll(&mut context) {
                    let actual = actual?;
                    assert_eq!(
                        ControlFlow::Break((actual.node, actual.source_order)),
                        result
                    );
                    break;
                }
                assert!(builder.storage.try_borrow_mut().is_ok());
                assert!(produced.get() <= consumed);
            }
            assert_eq!(produced.get(), consumed);
            assert_storage(&builder.storage.borrow(), &expected.storage);
        }
    }
    Ok(())
}
