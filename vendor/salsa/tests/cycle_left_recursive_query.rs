#![cfg(all(feature = "inventory", feature = "accumulator"))]

use crate::common::{ExecuteValidateLoggerDatabase, LogDatabase};
use expect_test::expect;
use salsa::{Database, Durability, Id};

mod common;

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_a(db: &dyn salsa::Database) -> Interned<'_> {
    let interned = query_b(db);
    let value = interned.value(db);

    if value < 10 {
        Interned::new(db, value + 1)
    } else {
        interned
    }
}

#[salsa::tracked(returns(copy))]
fn query_b(db: &dyn Database) -> Interned<'_> {
    let interned = query_a(db);
    query_x(db, interned);
    interned
}

#[salsa::tracked(returns(copy))]
fn query_x<'db>(db: &'db dyn Database, _i: Interned<'db>) {
    StableInput::get(db).value(db);
}

fn cycle_initial(db: &dyn Database, _id: Id) -> Interned<'_> {
    // Keep cycle-created values reusable so the test still covers validation ordering.
    db.report_untracked_read();
    Interned::new(db, 0)
}

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

#[test_log::test]
fn the_test() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let _ = StableInput::builder(())
        .durability(Durability::HIGH)
        .new(&db);

    let result = query_a(&db);

    assert_eq!(result.value(&db), 10);

    db.clear_logs();
    db.synthetic_write(Durability::HIGH);

    let result = query_a(&db);

    assert_eq!(result.value(&db), 10);

    // What this test captures is that the interned values **must** be validated before validating their corresponding `query_x` call.
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateInternedValue { key: Interned(Id(180)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(180)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(181)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(181)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(182)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(182)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(183)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(183)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(184)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(184)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(185)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(185)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(186)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(186)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(187)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(187)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(188)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(188)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(189)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(189)) })",
            "salsa_event(DidValidateInternedValue { key: Interned(Id(18a)), revision: R2 })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_x(Id(18a)) })",
            "salsa_event(DidValidateMemoizedValue { database_key: query_a(Id(80)) })",
        ]"#]]);
}
