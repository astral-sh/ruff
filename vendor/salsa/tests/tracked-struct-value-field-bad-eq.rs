#![cfg(feature = "inventory")]

//! Test a field whose `PartialEq` impl is always true.
//! This can result in us getting different results than
//! if we were to execute from scratch.

use expect_test::expect;
use salsa::{Database, Setter};
mod common;
use common::LogDatabase;
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: bool,
}

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Eq, Hash, Debug, Clone, salsa::SalsaValue)]
struct BadEq {
    field: bool,
}

impl PartialEq for BadEq {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl From<bool> for BadEq {
    fn from(value: bool) -> Self {
        Self { field: value }
    }
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[tracked]
    field: BadEq,
}

#[salsa::tracked(returns(copy))]
fn the_fn(db: &dyn Database, input: MyInput) -> bool {
    let tracked = make_tracked_struct(db, input);
    read_tracked_struct(db, tracked)
}

#[salsa::tracked(returns(copy))]
fn make_tracked_struct(db: &dyn Database, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, BadEq::from(input.field(db)))
}

#[salsa::tracked(returns(copy))]
fn read_tracked_struct<'db>(db: &'db dyn Database, tracked: MyTracked<'db>) -> bool {
    tracked.field(db).field
}

#[test]
fn execute() {
    let mut db = common::ExecuteValidateLoggerDatabase::default();

    let input = MyInput::new(&db, true);
    let result = the_fn(&db, input);
    assert!(result);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: the_fn(Id(0)) })",
            "salsa_event(WillExecute { database_key: make_tracked_struct(Id(0)) })",
            "salsa_event(WillExecute { database_key: read_tracked_struct(Id(80)) })",
        ]"#]]);

    // Update the input to `false` and re-execute.
    input.set_field(&mut db).to(false);
    let result = the_fn(&db, input);

    // If the `Eq` impl were working properly, we would
    // now return `false`. But because the `Eq` is considered
    // equal we re-use memoized results and so we get true.
    assert!(result);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: make_tracked_struct(Id(0)) })",
            "salsa_event(DidValidateMemoizedValue { database_key: read_tracked_struct(Id(80)) })",
            "salsa_event(DidValidateMemoizedValue { database_key: the_fn(Id(0)) })",
        ]"#]]);
}
