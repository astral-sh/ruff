#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Test that when having nested tracked functions
//! we don't drop any values when accumulating.

mod common;

use expect_test::expect;
use salsa::{Accumulator, Database, DatabaseImpl};
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
}

#[salsa::tracked(returns(copy))]
fn push_b_logs(db: &dyn Database) {
    // No logs
    push_c_logs(db);
}

#[salsa::tracked(returns(copy))]
fn push_c_logs(db: &dyn Database) {
    // No logs
    push_d_logs(db);
}

#[salsa::tracked(returns(copy))]
fn push_d_logs(db: &dyn Database) {
    Log("log d".to_string()).accumulate(db);
}

#[test]
fn accumulate_chain() {
    DatabaseImpl::new().attach(|db| {
        let logs = push_logs::accumulated::<Log>(db);
        // Check that we get all the logs.
        expect![[r#"
            [
                Log(
                    "log a",
                ),
                Log(
                    "log d",
                ),
            ]"#]]
        .assert_eq(&format!("{logs:#?}"));
    })
}
