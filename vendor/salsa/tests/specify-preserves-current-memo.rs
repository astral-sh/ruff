#![cfg(feature = "inventory")]

mod common;

use common::LogDatabase;
use expect_test::expect;
use salsa::{Database, Setter};

#[salsa::tracked]
struct Owner<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::input]
struct Input {
    #[returns(copy)]
    specify: bool,
}

#[salsa::input]
struct AssignInput {
    #[returns(copy)]
    later: u32,
}

#[salsa::tracked]
struct Child<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), specify)]
fn overridable<'db>(db: &'db dyn Database, owner: Owner<'db>) -> Child<'db> {
    Child::new(db, owner.value(db) + 10)
}

#[salsa::tracked(returns(copy))]
fn specify_over_derived_memo(db: &dyn Database, input: Input) -> (Child<'_>, Child<'_>) {
    let owner = Owner::new(db, 1);
    let derived = overridable(db, owner);
    if input.specify(db) {
        let replacement = Child::new(db, 99);
        overridable::specify(db, owner, replacement);
        (replacement, overridable(db, owner))
    } else {
        (derived, derived)
    }
}

#[salsa::tracked(returns(copy))]
fn assign_before_later_input(db: &dyn Database, input: AssignInput) -> (Owner<'_>, Child<'_>) {
    let owner = Owner::new(db, 2);
    let child = Child::new(db, 77);
    overridable::specify(db, owner, child);
    input.later(db);
    (owner, child)
}

#[test]
fn specify_preserves_current_derived_memo() {
    let mut db = common::DiscardLoggerDatabase::default();
    let input = Input::new(&db, false);

    let (returned, stored) = specify_over_derived_memo(&db, input);
    assert_eq!(returned.value(&db), 11);
    assert_eq!(stored.value(&db), 11);
    db.assert_logs(expect!["[]"]);

    input.set_specify(&mut db).to(true);

    let (returned, stored) = specify_over_derived_memo(&db, input);
    assert_eq!(returned.value(&db), 99);
    assert_eq!(stored.value(&db), 11);
    db.assert_logs(expect!["[]"]);

    db.synthetic_write(salsa::Durability::LOW);

    let (returned, stored) = specify_over_derived_memo(&db, input);
    assert_eq!(returned.value(&db), 99);
    assert_eq!(stored.value(&db), 11);
    db.assert_logs(expect!["[]"]);
}

#[test]
fn unconditional_specify_retains_output_ownership() {
    let mut db = common::DiscardLoggerDatabase::default();
    let input = AssignInput::new(&db, 0);

    let (owner, child) = assign_before_later_input(&db, input);
    assert!(overridable(&db, owner) == child);

    // Validation marks `overridable` current before discovering that `later` changed.
    // Re-execution must be able to call `specify` unconditionally without replacing the memo,
    // while still recording that `assign_before_later_input` owns the specified output.
    input.set_later(&mut db).to(1);
    let (owner, child) = assign_before_later_input(&db, input);
    assert!(overridable(&db, owner) == child);

    // If the no-op `specify` above failed to re-record its output, validating the outer query
    // would not validate `overridable`, which would then execute its one-shot implementation.
    db.synthetic_write(salsa::Durability::LOW);
    let (owner, child) = assign_before_later_input(&db, input);
    assert!(overridable(&db, owner) == child);
}
