#![cfg(feature = "inventory")]

//! A nested cycle where the inner cycle (`query_b`) stops depending on the outer cycle (`query_a`) after one iteration.

use crate::common::{ExecuteValidateLoggerDatabase, LogDatabase};
use expect_test::expect;

mod common;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

impl std::ops::Add for CycleValue {
    type Output = Self;

    fn add(self, other: CycleValue) -> CycleValue {
        CycleValue(self.0 + other.0)
    }
}

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(3);

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn salsa::Database) -> CycleValue {
    let b = query_b(db);
    let c = query_d(db);

    (b + c).min(MAX)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn salsa::Database) -> CycleValue {
    let c_value = query_c(db);

    if c_value == CycleValue(0) {
        query_a(db) + CycleValue(1)
    } else {
        c_value
    }
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_c(db: &dyn salsa::Database) -> CycleValue {
    let b_value = query_b(db);

    if b_value == CycleValue(0) {
        query_d(db)
    } else {
        b_value
    }
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn salsa::Database) -> CycleValue {
    let a_value = query_a(db);

    if a_value == CycleValue(0) {
        query_c(db)
    } else {
        a_value
    }
}

fn initial(_db: &dyn salsa::Database, _id: salsa::Id) -> CycleValue {
    MIN
}

#[test]
fn the_test() {
    let db = ExecuteValidateLoggerDatabase::default();
    let result = query_a(&db);

    assert_eq!(result, MAX);

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(80)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(100)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(180)) })",
            "salsa_event(WillIterateCycle { database_key: query_a(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: query_b(Id(80)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(100)) })",
            "salsa_event(DidFinalizeCycle { database_key: query_b(Id(80)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: query_d(Id(180)) })",
            "salsa_event(WillIterateCycle { database_key: query_a(Id(0)), iteration: 2 })",
            "salsa_event(WillExecute { database_key: query_d(Id(180)) })",
            "salsa_event(WillIterateCycle { database_key: query_a(Id(0)), iteration: 3 })",
            "salsa_event(WillExecute { database_key: query_d(Id(180)) })",
            "salsa_event(DidFinalizeCycle { database_key: query_a(Id(0)), iteration: 3 })",
        ]"#]]);
}
