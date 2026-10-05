use std::panic::AssertUnwindSafe;

use super::*;

fn fixture(point: RetainedPoint) -> TestDb {
    database(match point {
        RetainedPoint::Frames => {
            "class A: ...\nclass B(A): ...\nclass C(B): ...\nclass D(C): ...\nclass E(D): ...\nclass F(E): ...\nclass Product(F): ...\n"
        }
        RetainedPoint::Revisited => {
            "class A: ...\nclass B: ...\nclass C: ...\nclass Product(A, B, C, A, A, A): ...\n"
        }
    })
}

fn assert_drained(journal: &Journal) {
    assert_eq!(journal.live_traversals, 0);
    assert_eq!(journal.traversals, journal.dropped_traversals);
    assert_no_active_attempt();
}

fn first_point(journal: &Journal, point: RetainedPoint) -> TraversalSnapshot {
    *journal
        .snapshots
        .iter()
        .find(|snapshot| point.reached(snapshot))
        .unwrap()
}

fn retry<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    class: StaticClassLiteral<'db>,
) -> Journal {
    let revision = salsa::plumbing::current_revision(db);
    let recording = Recording::start(class);
    let retry = capture(db, || controlled(prepared, class)).unwrap();
    assert_eq!(retry.value, Ok(AnalysisOutcome::Complete(None)));
    let key = inheritance_cycle_inner_ingredient(db).database_key_index(class.as_id());
    assert_read(&retry.reads, retry.stamp, key);
    assert_drained(&recording.snapshot());
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    recording.snapshot()
}

/// Repeated inactive bases stabilize the visited set's retained backing bounds after the first duplicate.
/// That first duplicate can reserve before equality is checked. The active set inserts and pops each duplicate.
#[test]
fn repeated_inactive_bases_preserve_both_backing_bounds() {
    let db = fixture(RetainedPoint::Revisited);
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let recording = Recording::start(class);
    assert_eq!(
        controlled(&prepared, class),
        Ok(AnalysisOutcome::Complete(None))
    );
    let journal = recording.snapshot();
    let repeated = journal
        .snapshots
        .iter()
        .filter(|snapshot| RetainedPoint::Revisited.reached(snapshot))
        .collect::<Vec<_>>();
    assert_eq!(repeated.len(), 3);
    let full = repeated[0];
    for snapshot in repeated {
        assert_eq!(snapshot.visited_slots, full.visited_slots);
        assert_eq!(
            snapshot.visited_ordered_capacity,
            full.visited_ordered_capacity
        );
        assert_eq!(snapshot.active_slots, full.active_slots);
        assert_eq!(
            snapshot.active_ordered_capacity,
            full.active_ordered_capacity
        );
    }
    assert_drained(&journal);
}

/// Retained table and ordered-buffer bounds stay linear in the number of distinct visited bases.
/// Sixteen independent bases force several growth steps without increasing active-path depth.
#[test]
fn unique_bases_keep_retained_bounds_linear() {
    let mut source = String::new();
    for index in 0..16 {
        source.push_str(&format!("class Base{index}: ...\n"));
    }
    source.push_str("class Product(");
    source.push_str(
        &(0..16)
            .map(|index| format!("Base{index}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    source.push_str("): ...\n");
    let db = database(&source);
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let recording = Recording::start(class);
    assert_eq!(
        controlled(&prepared, class),
        Ok(AnalysisOutcome::Complete(None))
    );
    let journal = recording.snapshot();
    let inserted = journal
        .snapshots
        .iter()
        .filter(|snapshot| snapshot.after_enter)
        .collect::<Vec<_>>();
    assert_eq!(inserted.len(), 16);
    for (index, snapshot) in inserted.into_iter().enumerate() {
        assert_eq!(snapshot.visited, index + 1);
        assert_eq!(snapshot.active, 1);
        let bound = 16 * (snapshot.visited + 4);
        assert!(snapshot.visited_slots <= bound);
        assert!(snapshot.visited_ordered_capacity <= bound);
        assert!(snapshot.visited_slots >= snapshot.visited);
        assert!(snapshot.visited_ordered_capacity >= snapshot.visited);
    }
    assert_drained(&journal);
}

/// Work exhaustion after frame growth or an inactive-base revisit drops the traversal without publishing a cycle result.
/// Limits come from a separate fully funded run, and retry uses the normal policy in the same revision.
#[test]
fn retained_traversal_work_refusal_drains_and_retries() {
    for point in [RetainedPoint::Frames, RetainedPoint::Revisited] {
        let measured = fixture(point);
        let measured_prepared = prepare(&measured);
        let measured_class = cold_class(&measured, &measured_prepared);
        let recording = Recording::start(measured_class);
        assert_eq!(
            controlled(&measured_prepared, measured_class),
            Ok(AnalysisOutcome::Complete(None))
        );
        let snapshot = first_point(&recording.snapshot(), point);
        if matches!(point, RetainedPoint::Frames) {
            assert!(snapshot.frame_capacity >= 6);
        }
        assert_drained(&recording.snapshot());
        drop(recording);
        let limit = funded().semantic_work_limit - snapshot.remaining_work;

        let db = fixture(point);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(class);
        assert_eq!(
            controlled_member_operation(
                &prepared,
                Request(class),
                &AnalysisPolicy {
                    semantic_work_limit: limit,
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        let interrupted = recording.snapshot();
        assert_eq!(first_point(&interrupted, point).remaining_work, 0);
        assert_drained(&interrupted);
        assert_missing(&db, class);
        drop(recording);
        retry(&db, &prepared, class);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn reaches_with_bytes(point: RetainedPoint, bytes: usize) -> bool {
    let db = fixture(point);
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let recording = Recording::start(class);
    let result = controlled_member_operation(
        &prepared,
        Request(class),
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    );
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(None))
                | Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                })
        ),
        "{result:?}"
    );
    let journal = recording.snapshot();
    assert_drained(&journal);
    journal
        .snapshots
        .iter()
        .any(|snapshot| point.reached(snapshot))
}

/// Byte exhaustion drains retained traversal state without publishing a cycle result and permits same-revision retry.
/// The smallest budget reaching that state rejects a later real resource charge. Searching fresh
/// databases avoids reusing child memos while locating that boundary.
#[test]
fn retained_traversal_byte_refusal_drains_and_retries() {
    for point in [RetainedPoint::Frames, RetainedPoint::Revisited] {
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        assert!(reaches_with_bytes(point, upper));
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            if reaches_with_bytes(point, middle) {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        let db = fixture(point);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(class);
        assert_eq!(
            controlled_member_operation(
                &prepared,
                Request(class),
                &AnalysisPolicy {
                    requested_bytes_limit: lower,
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                completed: ()
            }),
        );
        let interrupted = recording.snapshot();
        first_point(&interrupted, point);
        assert_drained(&interrupted);
        assert_missing(&db, class);
        drop(recording);
        retry(&db, &prepared, class);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

/// Cancellation after retained-state growth or a revisit drains the traversal and permits same-revision retry.
/// Salsa finishes the active canonical query before delivering cancellation, so retry reuses its complete memo.
#[test]
fn cancelled_retained_traversals_drain_and_retry() {
    for point in [RetainedPoint::Frames, RetainedPoint::Revisited] {
        let db = fixture(point);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(class);
        recording.cancel_at(point);
        let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, class)));
        assert!(
            matches!(cancelled, Err(salsa::Cancelled::Local)),
            "{cancelled:?}"
        );
        let journal = recording.snapshot();
        first_point(&journal, point);
        assert!(journal.cancel_at.is_none());
        assert_drained(&journal);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                inheritance_cycle_inner_ingredient(&db),
                class.as_id()
            )
            .is_ok()
        );
        assert_eq!(class.inheritance_cycle(&db), None);
        drop(recording);
        assert_eq!(retry(&db, &prepared, class).bodies, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
