#![cfg(feature = "inventory")]

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use salsa::attempt_probe::{
    AttemptOutcome, Incomplete, charge, try_with_attempt, try_with_operation,
};
use salsa::prepared_source_probe::{
    CaptureError, PreparationError, Read, Stamp, Status, assert_no_active_attempt, capture,
    try_with_preparation,
};
use salsa::{Database, DatabaseImpl, Durability};

#[salsa::tracked(returns(copy), cycle_initial = |_, _| 0)]
fn productive_cycle(db: &dyn Database) -> u32 {
    (productive_cycle(db) + 1).min(3)
}

#[salsa::tracked(returns(copy))]
fn leaf(_db: &dyn Database) -> u32 {
    7
}

#[salsa::tracked(returns(copy))]
fn attempts_active_capture(db: &dyn Database) -> bool {
    let body_ran = Cell::new(false);
    let result = capture(db, || body_ran.set(true));
    matches!(result, Err(CaptureError::ActiveQuery)) && !body_ran.get()
}

#[salsa::tracked(returns(copy))]
fn panics_after_read(db: &dyn Database) -> u32 {
    leaf(db);
    panic!("diagnostic capture test panic");
}

#[test]
fn cycle_root_is_final_and_earlier_reads_remain_provisional() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let cold = capture(&db, || productive_cycle(&db))?;

    assert_eq!(cold.value, 3);
    assert_eq!(cold.check_root_reads(), Ok(()));
    assert!(matches!(
        cold.reads.last(),
        Some(Read {
            status: Status::Final,
            parent: None,
            ..
        })
    ));
    assert_eq!(
        cold.reads
            .iter()
            .filter(|read| read.parent.is_none())
            .count(),
        1
    );

    // These statuses were copied while recursive reads returned provisional values.
    let provisional_reads: Vec<_> = cold
        .reads
        .iter()
        .copied()
        .filter(|read| read.status == Status::Provisional)
        .collect();
    assert!(!provisional_reads.is_empty());

    let warm = capture(&db, || productive_cycle(&db))?;
    assert_eq!(warm.value, 3);
    assert_eq!(warm.check_root_reads(), Ok(()));
    assert_eq!(warm.reads.len(), 1);
    let final_read = warm.reads[0];
    assert_eq!(final_read.status, Status::Final);
    assert_eq!(final_read.parent, None);
    assert!(cold.reads.last().is_some_and(|read| {
        read.key == final_read.key && read.memo_address == final_read.memo_address
    }));

    // A later final read of the same key does not change what an earlier read observed.
    for read in provisional_reads {
        assert_eq!(read.key, final_read.key);
        assert_eq!(read.stamp, final_read.stamp);
        assert_eq!(read.parent, Some(final_read.key));
        assert_eq!(read.status, Status::Provisional);
    }
    Ok(())
}

#[test]
fn active_query_capture_is_rejected_before_running_its_body() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    assert!(attempts_active_capture(&db));

    let captured = capture(&db, || leaf(&db))?;
    assert_eq!(captured.value, 7);
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_eq!(captured.reads.len(), 1);
    Ok(())
}

#[test]
fn nested_capture_rejection_preserves_outer_reads() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let inner_body_ran = Cell::new(false);
    let outer = capture(&db, || {
        assert_eq!(leaf(&db), 7);
        let nested = capture(&db, || inner_body_ran.set(true));
        assert!(matches!(nested, Err(CaptureError::NestedCapture)));
        leaf(&db)
    })?;

    assert!(!inner_body_ran.get());
    assert_eq!(outer.value, 7);
    assert_eq!(outer.check_root_reads(), Ok(()));
    assert_eq!(outer.reads.len(), 2);
    assert!(
        outer
            .reads
            .iter()
            .all(|read| read.parent.is_none() && read.status == Status::Final)
    );

    let subsequent = capture(&db, || leaf(&db))?;
    assert_eq!(subsequent.check_root_reads(), Ok(()));
    assert_eq!(subsequent.reads.len(), 1);
    Ok(())
}

#[test]
fn foreign_database_reads_are_rejected() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let foreign = DatabaseImpl::default();
    let captured = capture(&db, || {
        leaf(&db);
        leaf(&foreign)
    })?;

    assert_eq!(captured.value, 7);
    assert!(captured.belongs_to(&db));
    assert!(!captured.belongs_to(&foreign));
    assert_eq!(
        captured.check_root_reads(),
        Err(CaptureError::ForeignDatabaseRead)
    );
    Ok(())
}

#[test]
fn stamp_distinguishes_database_revisions() -> Result<(), CaptureError> {
    let mut db = DatabaseImpl::default();
    let previous_stamp = {
        let captured = capture(&db, || leaf(&db))?;
        assert!(captured.belongs_to(&db));
        assert_eq!(captured.check_root_reads(), Ok(()));
        captured.stamp
    };

    db.synthetic_write(Durability::LOW);

    let captured = capture(&db, || leaf(&db))?;
    assert!(captured.belongs_to(&db));
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_ne!(captured.stamp, previous_stamp);
    assert!(
        captured
            .reads
            .iter()
            .all(|read| read.stamp == captured.stamp)
    );
    Ok(())
}

#[test]
fn panic_discards_partial_capture_and_restores_capture_state() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = capture(&db, || panics_after_read(&db));
    }));
    assert!(result.is_err());

    let captured = capture(&db, || leaf(&db))?;
    assert_eq!(captured.value, 7);
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_eq!(captured.reads.len(), 1);
    assert_eq!(captured.reads[0].parent, None);
    assert_eq!(captured.reads[0].status, Status::Final);
    Ok(())
}

#[test]
fn closure_without_query_reads_has_no_root_reads() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let captured = capture(&db, || 123)?;

    assert_eq!(captured.value, 123);
    assert!(captured.reads.is_empty());
    assert_eq!(captured.check_root_reads(), Err(CaptureError::NoRootReads));
    Ok(())
}

#[test]
fn root_read_check_does_not_certify_the_closure_output() -> Result<(), CaptureError> {
    let db = DatabaseImpl::default();
    let captured = capture(&db, || {
        leaf(&db);
        "unrelated output"
    })?;

    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_eq!(captured.value, "unrelated output");
    Ok(())
}

thread_local! {
    static SEMANTIC_ENTRIES: Cell<usize> = const { Cell::new(0) };
    static CANCELLING_STRUCTURAL_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn structural(_db: &dyn Database) -> u32 {
    11
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn semantic(_db: &dyn Database) -> u32 {
    SEMANTIC_ENTRIES.set(SEMANTIC_ENTRIES.get() + 1);
    13
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = |_, _| 0)]
fn cancelling_structural(db: &dyn Database) -> u32 {
    CANCELLING_STRUCTURAL_ENTRIES.set(CANCELLING_STRUCTURAL_ENTRIES.get() + 1);
    db.cancellation_token().cancel();
    // A cycle-enabled query masks local cancellation even when its execution is acyclic.
    db.unwind_if_revision_cancelled();
    17
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn attempts_active_preparation(db: &dyn Database) -> bool {
    let body_ran = Cell::new(false);
    let result = try_with_preparation(db, || body_ran.set(true));
    result == Err(PreparationError::ActiveQuery) && !body_ran.get()
}

#[test]
fn structural_preparation_preserves_stamp_and_returns_value() {
    let db = DatabaseImpl::default();
    let stamp = Stamp::current(&db);
    assert_eq!(try_with_preparation(&db, || structural(&db)), Ok(11));
    assert!(stamp.belongs_to(&db));
    assert_no_active_attempt();
}

#[test]
fn preparation_and_lazy_work_reject_active_and_refused_attempts() {
    for refused in [false, true] {
        for foreign in [false, true] {
            let db = DatabaseImpl::default();
            let other = DatabaseImpl::default();
            let target = if foreign { &other } else { &db };
            let body_ran = Cell::new(false);
            let result = try_with_attempt(&db, 0, || {
                if refused {
                    assert_eq!(charge(&db, 1), Err(Incomplete::Allowance));
                }
                assert_eq!(
                    try_with_preparation(target, || body_ran.set(true)),
                    Err(PreparationError::ActiveAttempt),
                );
                assert!(catch_unwind(assert_no_active_attempt).is_err());
            });

            assert_eq!(
                result,
                Ok(if refused {
                    AttemptOutcome::Incomplete(Incomplete::Allowance)
                } else {
                    AttemptOutcome::Complete(())
                }),
            );
            assert!(!body_ran.get());
            assert_no_active_attempt();
            assert_eq!(try_with_preparation(target, || structural(target)), Ok(11));
        }
    }
}

#[test]
fn preparation_rejects_active_queries_and_operation_scopes() {
    let db = DatabaseImpl::default();
    assert!(attempts_active_preparation(&db));

    let body_ran = Cell::new(false);
    assert_eq!(
        try_with_operation(&db, || {
            assert_no_active_attempt();
            try_with_preparation(&db, || body_ran.set(true))
        }),
        Ok(Err(PreparationError::ActiveOperation)),
    );
    assert_eq!(
        try_with_preparation(&db, || {
            assert_no_active_attempt();
            try_with_preparation(&db, || body_ran.set(true))
        }),
        Ok(Err(PreparationError::ActiveOperation)),
    );
    assert!(!body_ran.get());
    assert_eq!(try_with_preparation(&db, || structural(&db)), Ok(11));
}

#[test]
fn preparation_rejects_cold_and_warm_semantic_queries() {
    for warm in [false, true] {
        let db = DatabaseImpl::default();
        SEMANTIC_ENTRIES.set(0);
        if warm {
            assert_eq!(semantic(&db), 13);
        }
        let entries = SEMANTIC_ENTRIES.get();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                try_with_preparation(&db, || semantic(&db))
            }))
            .is_err()
        );
        assert_eq!(SEMANTIC_ENTRIES.get(), entries);
        assert_eq!(try_with_preparation(&db, || structural(&db)), Ok(11));
        assert_eq!(semantic(&db), 13);
    }
}

#[test]
fn panic_removes_preparation_operation() {
    let db = DatabaseImpl::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            try_with_preparation(&db, || {
                assert_eq!(structural(&db), 11);
                panic!("structural preparation test panic");
            })
        }))
        .is_err()
    );
    assert_eq!(try_with_preparation(&db, || structural(&db)), Ok(11));
    assert_eq!(
        try_with_attempt(&db, 0, || structural(&db)),
        Ok(AttemptOutcome::Complete(11)),
    );
}

#[test]
fn masked_local_cancellation_unwinds_preparation_and_allows_same_revision_retry() {
    let db = DatabaseImpl::default();
    let stamp = Stamp::current(&db);
    let token = db.cancellation_token();
    let body_finished = Cell::new(false);
    CANCELLING_STRUCTURAL_ENTRIES.set(0);

    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        try_with_preparation(&db, || {
            let value = cancelling_structural(&db);
            assert!(token.is_cancelled());
            body_finished.set(true);
            value
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(body_finished.get());
    assert!(!token.is_cancelled());
    assert!(stamp.belongs_to(&db));
    assert_eq!(
        try_with_preparation(&db, || cancelling_structural(&db)),
        Ok(17),
    );
    assert_eq!(CANCELLING_STRUCTURAL_ENTRIES.get(), 1);
    assert!(stamp.belongs_to(&db));
}
