#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Demonstrates that accumulation is done in the order
//! in which things were originally executed.

mod common;

use expect_test::expect;
use salsa::{Accumulator, Database};
use test_log::test;

#[salsa::accumulator]
#[derive(Debug)]
struct Log(#[allow(dead_code)] String);

#[salsa::tracked(returns(copy))]
fn push_logs(db: &dyn Database) {
    push_a_logs(db);
}

#[salsa::tracked(returns(copy))]
fn push_a_logs(db: &dyn Database) {
    Log("log a".to_string()).accumulate(db);
    push_b_logs(db);
    push_c_logs(db);
    push_d_logs(db);
}

#[salsa::tracked(returns(copy))]
fn push_b_logs(db: &dyn Database) {
    Log("log b".to_string()).accumulate(db);
    push_d_logs(db);
}

#[salsa::tracked(returns(copy))]
fn push_c_logs(db: &dyn Database) {
    Log("log c".to_string()).accumulate(db);
}

#[salsa::tracked(returns(copy))]
fn push_d_logs(db: &dyn Database) {
    Log("log d".to_string()).accumulate(db);
}

#[test]
fn accumulate_execution_order() {
    salsa::DatabaseImpl::new().attach(|db| {
        let logs = push_logs::accumulated::<Log>(db);
        // Check that we get logs in execution order
        expect![[r#"
            [
                Log(
                    "log a",
                ),
                Log(
                    "log b",
                ),
                Log(
                    "log d",
                ),
                Log(
                    "log c",
                ),
            ]"#]]
        .assert_eq(&format!("{logs:#?}"));
    })
}
