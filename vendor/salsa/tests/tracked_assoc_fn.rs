#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.
#![allow(warnings)]

use common::LogDatabase as _;
use expect_test::expect;

mod common;

trait TrackedTrait<'db> {
    type Output;

    fn tracked_trait_fn(db: &'db dyn salsa::Database, input: MyInput) -> Self::Output;
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
struct MyOutput<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
impl MyInput {
    #[salsa::tracked(returns(copy))]
    fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> Self {
        Self::new(db, 2 * input.field(db))
    }

    #[salsa::tracked(returns(ref))]
    fn tracked_fn_ref(db: &dyn salsa::Database, input: MyInput) -> Self {
        Self::new(db, 3 * input.field(db))
    }
}

#[salsa::tracked]
impl<'db> TrackedTrait<'db> for MyOutput<'db> {
    type Output = Self;

    #[salsa::tracked(returns(copy))]
    fn tracked_trait_fn(db: &'db dyn salsa::Database, input: MyInput) -> Self::Output {
        Self::new(db, 4 * input.field(db))
    }
}

// The self-type of a tracked impl doesn't have to be tracked itself:
struct UntrackedHelper;

#[salsa::tracked]
impl<'db> TrackedTrait<'db> for UntrackedHelper {
    type Output = MyOutput<'db>;

    #[salsa::tracked(returns(copy))]
    fn tracked_trait_fn(db: &'db dyn salsa::Database, input: MyInput) -> Self::Output {
        MyOutput::tracked_trait_fn(db, input)
    }
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let output = MyOutput::tracked_trait_fn(&db, input);
    let helper_output = UntrackedHelper::tracked_trait_fn(&db, input);
    // assert_eq!(object.tracked_fn(&db), 44);
    // assert_eq!(*object.tracked_fn_ref(&db), 66);
    assert_eq!(output.field(&db), 88);
    assert_eq!(helper_output.field(&db), 88);
}

#[test]
fn debug_name() {
    let mut db = common::ExecuteValidateLoggerDatabase::default();
    let input = MyInput::new(&db, 22);
    let output = MyOutput::tracked_trait_fn(&db, input);

    assert_eq!(output.field(&db), 88);
    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: MyOutput < 'db >::tracked_trait_fn_(Id(0)) })",
        ]"#]]);
}
