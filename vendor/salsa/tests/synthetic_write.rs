#![cfg(feature = "inventory")]

//! Test that a constant `tracked` fn (has no inputs)
//! compiles and executes successfully.
#![allow(warnings)]

mod common;

use common::{LogDatabase, Logger};
use expect_test::expect;
use salsa::{Database, DatabaseImpl, Durability, Event, EventKind};

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn Database, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[test]
fn execute() {
    let mut db = common::ExecuteValidateLoggerDatabase::default();

    let input = MyInput::new(&db, 22);
    assert_eq!(tracked_fn(&db, input), 44);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: tracked_fn(Id(0)) })",
        ]"#]]);

    // Bumps the revision
    db.synthetic_write(Durability::LOW);

    // Query should re-run
    assert_eq!(tracked_fn(&db, input), 44);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateMemoizedValue { database_key: tracked_fn(Id(0)) })",
        ]"#]]);
}
