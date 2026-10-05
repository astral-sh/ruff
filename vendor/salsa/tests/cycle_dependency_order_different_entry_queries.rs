#![cfg(all(feature = "inventory", feature = "accumulator"))]

use crate::common::{ExecuteValidateLoggerDatabase, LogDatabase};
use expect_test::expect;
use salsa::{Database, Durability};

mod common;

#[salsa::tracked(returns(copy), cycle_initial=a_cycle_initial)]
fn query_a(db: &dyn Database) {
    let b = query_b(db);
    query_d(db, b);
}

fn a_cycle_initial(_db: &dyn Database, _id: salsa::Id) {}

#[salsa::interned]
struct Interned {
    #[returns(copy)]
    value: u32,
}

#[salsa::input(singleton)]
struct StableInput {
    #[returns(copy)]
    value: (),
}

#[salsa::tracked(returns(copy), cycle_initial=|db, _| Interned::new(db, 0))]
fn query_b(db: &dyn Database) -> Interned<'_> {
    query_c(db);
    // Keep this value reusable so the test still covers validation ordering.
    db.report_untracked_read();
    Interned::new(db, 2)
}

#[salsa::tracked(returns(copy))]
fn query_c(db: &dyn Database) {
    query_a(db);
}

#[salsa::tracked(returns(copy))]
fn query_d<'db>(db: &'db dyn Database, _i: Interned<'db>) {
    StableInput::get(db).value(db);
}

#[test_log::test]
fn the_test() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let _ = StableInput::builder(())
        .durability(Durability::HIGH)
        .new(&db);

    // We compute the result starting from query a...
    query_a(&db);

    db.clear_logs();
    db.synthetic_write(Durability::HIGH);

    // ...but we now verify query_b
    query_b(&db);

    // What this test captures is that `Interned(Id(c00))` must be verified **before** `query_d(Id(c00))`
    // as we would when starting from `query_a`
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateInternedValue { key: query_b::interned_arguments(Id(100)), revision: R2 })",
            "salsa_event(WillExecute { database_key: query_b(Id(100)) })",
            "salsa_event(DidValidateInternedValue { key: query_c::interned_arguments(Id(180)), revision: R2 })",
            "salsa_event(WillExecute { database_key: query_c(Id(180)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(200)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_d(Id(200)) })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_a(Id(80)) })",
        ]"#]]);
}
