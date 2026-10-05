#![cfg(feature = "inventory")]

//! A nested cycle where the inner cycle (`query_c`) never converges but `query_a` only depends on it in the first iteration.

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

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn salsa::Database) -> CycleValue {
    let b = query_b(db);

    tracing::info!("query_b: {b:?}");
    if b < CycleValue(1) { query_c(db) } else { b }
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn salsa::Database) -> CycleValue {
    query_a(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_c(db: &dyn salsa::Database) -> CycleValue {
    let a_value = query_a(db);
    let d_value = query_d(db);

    a_value + d_value + CycleValue(1)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn salsa::Database) -> CycleValue {
    query_c(db)
}

fn initial(_db: &dyn salsa::Database, _id: salsa::Id) -> CycleValue {
    MIN
}

#[test]
fn the_test() {
    let db = ExecuteValidateLoggerDatabase::default();
    let result = query_a(&db);

    assert_eq!(result, CycleValue(1));

    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: query_a(Id(0)) })",
            "salsa_event(WillExecute { database_key: query_b(Id(80)) })",
            "salsa_event(WillExecute { database_key: query_c(Id(100)) })",
            "salsa_event(WillExecute { database_key: query_d(Id(180)) })",
            "salsa_event(WillIterateCycle { database_key: query_a(Id(0)), iteration: 1 })",
            "salsa_event(WillExecute { database_key: query_b(Id(80)) })",
            "salsa_event(DidFinalizeCycle { database_key: query_a(Id(0)), iteration: 1 })",
        ]"#]]);
}
