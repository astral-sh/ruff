#![cfg(feature = "inventory")]
#![allow(warnings)]

use common::{EventLoggerDatabase, HasLogger, LogDatabase, Logger};
use expect_test::expect;
use salsa::plumbing::HasStorage;
use salsa::{Database, Durability, Event, EventKind, Setter};

mod common;
#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[test]
fn execute() {
    let mut db = EventLoggerDatabase::default();
    let input_low = MyInput::new(&db, 22);
    let input_high = MyInput::builder(2200).durability(Durability::HIGH).new(&db);

    assert_eq!(tracked_fn(&db, input_low), 44);
    assert_eq!(tracked_fn(&db, input_high), 4400);

    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: tracked_fn(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: tracked_fn(Id(1)) }",
        ]"#]]);

    db.synthetic_write(Durability::LOW);

    assert_eq!(tracked_fn(&db, input_low), 44);
    assert_eq!(tracked_fn(&db, input_high), 4400);

    // FIXME: There's currently no good way to verify whether an input was validated using shallow or deep comparison.
    // All we can do for now is verify that the values were validated.
    // Note: It maybe confusing why it validates `input_high` when the write has `Durability::LOW`.
    // This is because all values must be validated whenever a write occurs. It doesn't mean that it
    // executed the query.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "DidValidateMemoizedValue { database_key: tracked_fn(Id(0)) }",
            "WillCheckCancellation",
            "DidValidateMemoizedValue { database_key: tracked_fn(Id(1)) }",
        ]"#]]);
}
