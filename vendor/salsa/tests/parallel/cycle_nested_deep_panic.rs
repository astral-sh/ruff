// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! Tests that salsa doesn't get stuck after a panic in a nested cycle function.

use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};
use std::fmt;
use std::panic::catch_unwind;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(3);

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    let c_value = query_c(db);
    CycleValue(c_value.0 + 1).min(MAX)
}

#[salsa::tracked(returns(copy))]
fn query_c(db: &dyn KnobsDatabase) -> CycleValue {
    let d_value = query_d(db);

    if d_value > CycleValue(0) {
        let e_value = query_e(db);
        let b_value = query_b(db);
        CycleValue(d_value.0.max(e_value.0).max(b_value.0))
    } else {
        let a_value = query_a(db);
        CycleValue(d_value.0.max(a_value.0))
    }
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn KnobsDatabase) -> CycleValue {
    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_e(db: &dyn KnobsDatabase) -> CycleValue {
    query_c(db)
}

fn initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    MIN
}

fn run() {
    tracing::debug!("Starting new run");
    let db_t1 = Knobs::default();
    let db_t2 = db_t1.clone();
    let db_t3 = db_t1.clone();
    let db_t4 = db_t1.clone();

    let t1 = thread::spawn(move || {
        let _span = tracing::debug_span!("t1", thread_id = ?thread::current().id()).entered();
        catch_unwind(|| {
            db_t1.wait_for(1);
            query_a(&db_t1)
        })
    });
    let t2 = thread::spawn(move || {
        let _span = tracing::debug_span!("t2", thread_id = ?thread::current().id()).entered();
        catch_unwind(|| {
            db_t2.wait_for(1);

            query_b(&db_t2)
        })
    });
    let t3 = thread::spawn(move || {
        let _span = tracing::debug_span!("t3", thread_id = ?thread::current().id()).entered();
        catch_unwind(|| {
            db_t3.signal(2);
            query_d(&db_t3)
        })
    });

    let r_t1 = t1.join().unwrap();
    let r_t2 = t2.join().unwrap();
    let r_t3 = t3.join().unwrap();

    assert_is_set_cycle_error(r_t1);
    assert_is_set_cycle_error(r_t2);
    assert_is_set_cycle_error(r_t3);

    // Pulling the cycle again at a later point should still result in a panic.
    assert_is_set_cycle_error(catch_unwind(|| query_d(&db_t4)));
}

#[test_log::test]
fn the_test() {
    let count = if cfg!(miri) { 1 } else { 200 };

    for _ in 0..count {
        run()
    }
}

#[track_caller]
fn assert_is_set_cycle_error<T>(result: Result<T, Box<dyn std::any::Any + Send>>)
where
    T: fmt::Debug,
{
    let err = result.expect_err("expected an error");

    if let Some(message) = err.downcast_ref::<&str>() {
        assert!(
            message.contains("set cycle_fn/cycle_initial to fixpoint iterate"),
            "Expected error message to contain 'set cycle_fn/cycle_initial to fixpoint iterate', but got: {}",
            message
        );
    } else if let Some(message) = err.downcast_ref::<String>() {
        assert!(
            message.contains("set cycle_fn/cycle_initial to fixpoint iterate"),
            "Expected error message to contain 'set cycle_fn/cycle_initial to fixpoint iterate', but got: {}",
            message
        );
    } else if err.downcast_ref::<salsa::Cancelled>().is_some() {
        // This is okay, because Salsa throws a Cancelled::PropagatedPanic when a panic occurs in a query
        // that it blocks on.
    } else {
        std::panic::resume_unwind(err);
    }
}
