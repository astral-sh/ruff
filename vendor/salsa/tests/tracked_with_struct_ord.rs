#![cfg(feature = "inventory")]

//! Test that `PartialOrd` and `Ord` can be derived for tracked structs

use salsa::{Database, DatabaseImpl};
use test_log::test;

#[salsa::input]
#[derive(PartialOrd, Ord)]
struct Input {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(debug)]
#[derive(Ord, PartialOrd)]
struct MyTracked<'db> {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(copy))]
fn create_tracked(db: &dyn Database, input: Input) -> MyTracked<'_> {
    MyTracked::new(db, input.value(db))
}

#[test]
fn execute() {
    DatabaseImpl::new().attach(|db| {
        let input1 = Input::new(db, 20);
        let input2 = Input::new(db, 10);

        // Compares by ID and not by value.
        assert!(input1 <= input2);

        let t0: MyTracked = create_tracked(db, input1);
        let t1: MyTracked = create_tracked(db, input2);

        assert!(t0 <= t1);
    })
}
