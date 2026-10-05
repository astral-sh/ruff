#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.
#![allow(warnings)]

use common::LogDatabase as _;
use expect_test::expect;

mod common;

trait TrackedTrait {
    fn tracked_trait_fn(self, db: &dyn salsa::Database) -> &u32;
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
impl MyInput {
    #[salsa::tracked]
    fn tracked_fn(self, db: &dyn salsa::Database) -> u32 {
        self.field(db) * 2
    }

    #[salsa::tracked(returns(ref))]
    fn tracked_fn_ref(self, db: &dyn salsa::Database) -> u32 {
        self.field(db) * 3
    }

    #[salsa::tracked]
    fn tracked_unit(self, _db: &dyn salsa::Database) {}
}

#[salsa::tracked]
impl TrackedTrait for MyInput {
    #[salsa::tracked]
    fn tracked_trait_fn(self, db: &dyn salsa::Database) -> u32 {
        self.field(db) * 4
    }
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let object = MyInput::new(&mut db, 22);
    assert_eq!(*object.tracked_fn(&db), 44);
    assert_eq!(*object.tracked_fn_ref(&db), 66);
    assert_eq!(*object.tracked_trait_fn(&db), 88);
    let _: &() = object.tracked_unit(&db);
}

#[test]
fn debug_name() {
    let mut db = common::ExecuteValidateLoggerDatabase::default();
    let object = MyInput::new(&mut db, 22);

    assert_eq!(*object.tracked_trait_fn(&db), 88);
    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: MyInput::tracked_trait_fn_(Id(0)) })",
        ]"#]]);
}
