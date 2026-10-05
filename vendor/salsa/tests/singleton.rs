#![cfg(feature = "inventory")]

//! Basic Singleton struct test:
//!
//! Singleton structs are created only once. Subsequent `get`s and `new`s after creation return the same `Id`.

use expect_test::expect;
use salsa::Database as _;
use test_log::test;

#[salsa::input(singleton, debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
    #[returns(copy)]
    id_field: u16,
}

#[test]
fn basic() {
    let db = salsa::DatabaseImpl::new();
    let input1 = MyInput::new(&db, 3, 4);
    let input2 = MyInput::get(&db);

    assert_eq!(input1, input2);

    let input3 = MyInput::try_get(&db);
    assert_eq!(Some(input1), input3);
}

#[test]
#[should_panic]
fn twice() {
    let db = salsa::DatabaseImpl::new();
    let input1 = MyInput::new(&db, 3, 4);
    let input2 = MyInput::get(&db);

    assert_eq!(input1, input2);

    // should panic here
    _ = MyInput::new(&db, 3, 5);
}

#[test]
fn debug() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = MyInput::new(db, 3, 4);
        let actual = format!("{input:?}");
        let expected = expect!["MyInput { [salsa id]: Id(0), field: 3, id_field: 4 }"];
        expected.assert_eq(&actual);
    });
}
