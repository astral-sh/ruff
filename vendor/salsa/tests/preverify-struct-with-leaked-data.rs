#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.

use std::cell::Cell;

use common::LogDatabase;
use expect_test::expect;
mod common;
use salsa::{Database, Setter};
use test_log::test;

thread_local! {
    static COUNTER: Cell<usize> = const { Cell::new(0) };
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field1: u32,
    #[returns(copy)]
    field2: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[tracked]
    #[returns(copy)]
    counter: usize,
}

#[salsa::tracked(returns(copy))]
fn function(db: &dyn Database, input: MyInput) -> (usize, usize) {
    // Read input 1
    let _field1 = input.field1(db);

    // **BAD:** Leak in the value of the counter non-deterministically
    let counter = COUNTER.with(|c| c.get());

    // Create the tracked struct, which (from salsa's POV), only depends on field1;
    // but which actually depends on the leaked value.
    let tracked = MyTracked::new(db, counter);

    // Read the tracked field
    let result = counter_field(db, tracked);

    // Read input 2. This will cause us to re-execute on revision 2.
    let _field2 = input.field2(db);

    (result, tracked.counter(db))
}

#[salsa::tracked(returns(copy))]
fn counter_field<'db>(db: &'db dyn Database, tracked: MyTracked<'db>) -> usize {
    tracked.counter(db)
}

#[test]
fn test_leaked_inputs_ignored() {
    let mut db = common::EventLoggerDatabase::default();

    let input = MyInput::new(&db, 10, 20);
    let result_in_rev_1 = function(&db, input);
    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: function(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: counter_field(Id(80)) }",
        ]"#]]);

    assert_eq!(result_in_rev_1, (0, 0));

    // Modify field2 so that `function` is seen to have changed --
    // but only *after* the tracked struct is created.
    input.set_field2(&mut db).to(30);

    // Also modify the thread-local counter
    COUNTER.with(|c| c.set(100));

    let result_in_rev_2 = function(&db, input);
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "DidValidateMemoizedValue { database_key: counter_field(Id(80)) }",
            "WillExecute { database_key: function(Id(0)) }",
            "WillCheckCancellation",
        ]"#]]);

    // Because salsa does not see any way for the tracked
    // struct to have changed, it will re-use the cached return value
    // from `counter_field` (`0`). This in turn "locks" the cached
    // struct so that the new value of 100 is ignored.
    //
    // Contrast with preverify-struct-with-leaked-data-2.rs.
    assert_eq!(result_in_rev_2, (0, 0));
}
