#![cfg(feature = "inventory")]

//! A value specified by a fixpoint cycle remains provisional until the cycle converges.

mod common;

use common::{ExecuteValidateLoggerDatabase, LogDatabase};
use expect_test::expect;

#[salsa::tracked]
struct Item<'db> {
    #[returns(copy)]
    value: (),
}

#[salsa::tracked(returns(copy), specify)]
fn specified(db: &dyn salsa::Database, item: Item<'_>) -> u32 {
    item.value(db);
    0
}

#[salsa::tracked(returns(copy))]
fn read_specified(db: &dyn salsa::Database, item: Item<'_>) -> u32 {
    specified(db, item)
}

#[salsa::tracked(returns(copy), cycle_initial = initial)]
fn cycle(db: &dyn salsa::Database) -> Option<Item<'_>> {
    let item = cycle(db).unwrap_or_else(|| Item::new(db, ()));

    specified::specify(db, item, 42);
    assert_eq!(read_specified(db, item), 42);
    Some(item)
}

fn initial(_db: &dyn salsa::Database, _id: salsa::Id) -> Option<Item<'_>> {
    None
}

#[test]
fn specified_value_inherits_cycle_heads() {
    let db = ExecuteValidateLoggerDatabase::default();

    assert!(cycle(&db).is_some());

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: cycle(Id(0)) })",
            "salsa_event(WillExecute { database_key: read_specified(Id(80)) })",
            "salsa_event(WillIterateCycle { database_key: cycle(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: read_specified(Id(80)) })",
            "salsa_event(DidFinalizeCycle { database_key: cycle(Id(0)), iteration: 1 })",
        ]"#]]);
}
