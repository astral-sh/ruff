#![cfg(feature = "inventory")]
//! Test that `DeriveWithDb` is correctly derived.

mod common;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Barrier,
    thread,
};

use expect_test::expect;
use salsa::{Cancelled, Database};

use crate::common::LogDatabase;

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn a(db: &dyn Database, input: MyInput) -> u32 {
    BARRIER.wait();
    BARRIER2.wait();
    b(db, input)
}
#[salsa::tracked(returns(copy))]
fn b(db: &dyn Database, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::tracked(returns(copy), cycle_initial = |_, _| 0)]
fn panicking_cycle_query(_db: &dyn Database) -> u32 {
    panic!("boom")
}

#[salsa::tracked(returns(copy))]
fn uncancelled_query(_db: &dyn Database) -> u32 {
    1
}

#[salsa::tracked(returns(copy))]
fn cancel_after_cycle_panic(db: &dyn Database) -> u32 {
    assert!(catch_unwind(AssertUnwindSafe(|| panicking_cycle_query(db))).is_err());
    db.cancellation_token().cancel();
    uncancelled_query(db)
}

static BARRIER: Barrier = Barrier::new(2);
static BARRIER2: Barrier = Barrier::new(2);

#[test]
fn cancellation_token() {
    let db = common::EventLoggerDatabase::default();
    let token = db.cancellation_token();
    let input = MyInput::new(&db, 22);
    let res = Cancelled::catch(|| {
        thread::scope(|s| {
            s.spawn(|| {
                BARRIER.wait();
                token.cancel();
                BARRIER2.wait();
            });
            a(&db, input)
        })
    });
    assert!(matches!(res, Err(Cancelled::Local)), "{res:?}");
    drop(res);
    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: a(Id(0)) }",
            "WillCheckCancellation",
        ]"#]]);
    thread::spawn(|| {
        BARRIER.wait();
        BARRIER2.wait();
    });
    a(&db, input);
    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: a(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: b(Id(0)) }",
        ]"#]]);
}

#[test]
fn cancellation_is_restored_after_cycle_panic() {
    let db = common::LoggerDatabase::default();
    let result = Cancelled::catch(|| cancel_after_cycle_panic(&db));
    assert!(matches!(result, Err(Cancelled::Local)), "{result:?}");
}
