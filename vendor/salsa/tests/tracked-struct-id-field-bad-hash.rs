#![cfg(feature = "inventory")]

//! Test for a tracked struct where an untracked field has a
//! very poorly chosen hash impl (always returns 0).
//!
//! This demonstrates that tracked struct ids will always change if
//! untracked fields on a struct change values, because although struct
//! ids are based on the *hash* of the untracked fields, ids are generational
//! based on the field values.

use salsa::{Database as Db, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u64,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, salsa::SalsaValue)]
struct BadHash {
    field: u64,
}

impl From<u64> for BadHash {
    fn from(value: u64) -> Self {
        Self { field: value }
    }
}

impl std::hash::Hash for BadHash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_i16(0);
    }
}

#[salsa::tracked]
struct MyTracked<'db> {
    field: BadHash,
}

#[salsa::tracked(returns(copy))]
fn the_fn(db: &dyn Db, input: MyInput) {
    let tracked0 = MyTracked::new(db, BadHash::from(input.field(db)));
    assert_eq!(tracked0.field(db).field, input.field(db));
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, 1);
    the_fn(&db, input);
    input.set_field(&mut db).to(0);
    the_fn(&db, input);
}

#[salsa::tracked(returns(copy))]
fn create_tracked(db: &dyn Db, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, BadHash::from(input.field(db)))
}

#[salsa::tracked(returns(copy))]
fn with_tracked<'db>(db: &'db dyn Db, tracked: MyTracked<'db>) -> u64 {
    tracked.field(db).field
}

#[test]
fn dependent_query() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, 1);
    let tracked = create_tracked(&db, input);
    assert_eq!(with_tracked(&db, tracked), 1);

    input.set_field(&mut db).to(0);

    // We now re-run the query that creates the tracked struct.
    //
    // Salsa will re-use the `MyTracked` struct from the previous revision,
    // but practically it has been re-created due to generational ids.
    let tracked = create_tracked(&db, input);
    assert_eq!(with_tracked(&db, tracked), 0);
    input.set_field(&mut db).to(2);
    let tracked = create_tracked(&db, input);
    assert_eq!(with_tracked(&db, tracked), 2);
}
