#![cfg(feature = "inventory")]

//! Test that creating a tracked struct outside of a
//! tracked function panics with an assert message.

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    field: u32,
}

#[test]
#[should_panic(
    expected = "cannot create a tracked struct disambiguator outside of a tracked function"
)]
fn execute() {
    let db = salsa::DatabaseImpl::new();
    MyTracked::new(&db, 0);
}
