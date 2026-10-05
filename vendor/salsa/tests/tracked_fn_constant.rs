#![cfg(feature = "inventory")]

//! Test that a constant `tracked` fn (has no inputs)
//! compiles and executes successfully.
#![allow(warnings)]

use crate::common::LogDatabase;

mod common;

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn salsa::Database) -> u32 {
    44
}

#[salsa::tracked(returns(copy))]
fn tracked_custom_db(db: &dyn LogDatabase) -> u32 {
    44
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    assert_eq!(tracked_fn(&db), 44);
}

#[test]
fn execute_custom() {
    let mut db = common::LoggerDatabase::default();
    assert_eq!(tracked_custom_db(&db), 44);
}
