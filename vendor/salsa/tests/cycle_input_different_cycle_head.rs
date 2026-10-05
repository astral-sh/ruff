#![cfg(feature = "inventory")]

//! Tests that the durability correctly propagates
//! to all cycle heads.

use salsa::Setter as _;

#[test_log::test]
fn low_durability_cycle_enter_from_different_head() {
    let mut db = MyDbImpl::default();
    // Start with 0, the same as returned by cycle initial
    let input = Input::builder(0).new(&db);
    db.input = Some(input);

    assert_eq!(query_a(&db), 0); // Prime the Db

    input.set_value(&mut db).to(10);

    assert_eq!(query_b(&db), 10);
}

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[salsa::db]
trait MyDb: salsa::Database {
    fn input(&self) -> Input;
}

#[salsa::db]
#[derive(Clone, Default)]
struct MyDbImpl {
    storage: salsa::Storage<Self>,
    input: Option<Input>,
}

#[salsa::db]
impl salsa::Database for MyDbImpl {}

#[salsa::db]
impl MyDb for MyDbImpl {
    fn input(&self) -> Input {
        self.input.unwrap()
    }
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_a(db: &dyn MyDb) -> u32 {
    query_b(db);
    db.input().value(db)
}

fn cycle_initial(_db: &dyn MyDb, _id: salsa::Id) -> u32 {
    0
}

#[salsa::interned]
struct Interned {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_b(db: &dyn MyDb) -> u32 {
    query_c(db)
}

#[salsa::tracked(returns(copy))]
fn query_c(db: &dyn MyDb) -> u32 {
    query_a(db)
}
