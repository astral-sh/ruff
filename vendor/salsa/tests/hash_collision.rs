#![cfg(feature = "inventory")]

use std::hash::Hash;

#[test]
fn hello() {
    use salsa::{Database, DatabaseImpl, Setter};

    #[salsa::input]
    struct Bool {
        #[returns(copy)]
        value: bool,
    }

    #[salsa::tracked]
    struct True<'db> {}

    #[salsa::tracked]
    struct False<'db> {}

    #[salsa::tracked(returns(copy))]
    fn hello(db: &dyn Database, bool: Bool) {
        if bool.value(db) {
            True::new(db);
        } else {
            False::new(db);
        }
    }

    let mut db = DatabaseImpl::new();
    let input = Bool::new(&db, false);
    hello(&db, input);
    input.set_value(&mut db).to(true);
    hello(&db, input);
}
