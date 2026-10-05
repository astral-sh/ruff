#![cfg(feature = "inventory")]

//! Basic deletion test:
//!
//! * entities not created in a revision are deleted, as is any memoized data keyed on them.

mod common;

use salsa::{Database, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    identity: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    identifier: u32,

    #[tracked]
    #[returns(ref)]
    field: Bomb,
}

thread_local! {
    static DROPPED: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(vec![]) };
}

fn dropped() -> Vec<u32> {
    DROPPED.with(|d| d.borrow().clone())
}

#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
struct Bomb {
    identity: u32,
}

impl Drop for Bomb {
    fn drop(&mut self) {
        DROPPED.with(|d| d.borrow_mut().push(self.identity));
    }
}

#[salsa::tracked]
impl MyInput {
    #[salsa::tracked(returns(copy))]
    fn create_tracked_struct(self, db: &dyn Database) -> MyTracked<'_> {
        MyTracked::new(
            db,
            self.identity(db),
            Bomb {
                identity: self.identity(db),
            },
        )
    }
}

#[test]
fn deletion_drops() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, 22);

    expect_test::expect![[r#"
        []
    "#]]
    .assert_debug_eq(&dropped());

    let tracked_struct = input.create_tracked_struct(&db);
    assert_eq!(tracked_struct.field(&db).identity, 22);

    expect_test::expect![[r#"
        []
    "#]]
    .assert_debug_eq(&dropped());

    input.set_identity(&mut db).to(44);

    expect_test::expect![[r#"
        []
    "#]]
    .assert_debug_eq(&dropped());

    // Now that we execute with rev = 44, the old id is put on the free list
    let tracked_struct = input.create_tracked_struct(&db);
    assert_eq!(tracked_struct.field(&db).identity, 44);

    expect_test::expect![[r#"
        []
    "#]]
    .assert_debug_eq(&dropped());

    // When we execute again with `input1`, that id is re-used, so the old value is deleted
    let input1 = MyInput::new(&db, 66);
    let _tracked_struct1 = input1.create_tracked_struct(&db);

    expect_test::expect![[r#"
        [
            22,
        ]
    "#]]
    .assert_debug_eq(&dropped());
}
