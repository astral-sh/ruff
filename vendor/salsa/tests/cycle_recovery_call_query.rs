#![cfg(feature = "inventory")]

//! It's possible to call a Salsa query from within a cycle recovery fn.

use salsa::Setter as _;

#[salsa::tracked(returns(copy))]
fn fallback_value(_db: &dyn salsa::Database) -> u32 {
    10
}

#[salsa::tracked(returns(copy), cycle_fn = |db, _, _, _| fallback_value(db), cycle_initial = |_, _| 0)]
fn query(db: &dyn salsa::Database) -> u32 {
    let val = query(db);
    if val < 5 { val + 1 } else { val }
}

#[test_log::test]
fn the_test() {
    let db = salsa::DatabaseImpl::default();

    assert_eq!(query(&db), 10);
}

#[salsa::input]
struct RecoveryInput {
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn recovery_dependency(db: &dyn salsa::Database, input: RecoveryInput) -> u32 {
    let value = *input.value(db);
    assert_ne!(value, 0, "recovery dependency panicked");
    value
}

#[salsa::tracked(
    returns(copy),
    cycle_fn = |db, _, _, _, input| recovery_dependency(db, input),
    cycle_initial = |_, _, _| 0,
)]
fn query_with_recovery_dependency(db: &dyn salsa::Database, input: RecoveryInput) -> u32 {
    let value = query_with_recovery_dependency(db, input);
    if value < 5 { value + 1 } else { value }
}

#[salsa::tracked(returns(copy))]
fn recovery_entry(db: &dyn salsa::Database, input: RecoveryInput) -> u32 {
    query_with_recovery_dependency(db, input)
}

#[test]
fn recovery_query_dependency_changes_in_later_revision() {
    let mut db = salsa::DatabaseImpl::default();
    let input = RecoveryInput::new(&db, 10);
    assert_eq!(recovery_entry(&db, input), 10);

    input.set_value(&mut db).to(20);
    assert_eq!(recovery_entry(&db, input), 20);
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn recovery_query_panic_releases_active_frames_and_claims() {
    let mut db = salsa::DatabaseImpl::default();
    let input = RecoveryInput::new(&db, 0);
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| recovery_entry(&db, input)));
    assert!(result.is_err());

    input.set_value(&mut db).to(10);
    assert_eq!(recovery_entry(&db, input), 10);
}
