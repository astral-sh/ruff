#![cfg(feature = "inventory")]

//! Test that a setting a field on a `#[salsa::input]`
//! overwrites and returns the old value.

use salsa::{Database, DatabaseImpl, SalsaValue};
use test_log::test;

#[salsa::input(debug)]
struct MyInput {
    field: String,
}

#[salsa::tracked(debug)]
struct MyTracked<'db> {
    #[tracked]
    #[returns(copy)]
    data: MyInput,
    #[tracked]
    next: MyList<'db>,
}

#[derive(PartialEq, Eq, Clone, Debug, SalsaValue)]
enum MyList<'db> {
    None,
    Next(MyTracked<'db>),
}

#[salsa::tracked(returns(copy))]
fn create_tracked_list(db: &dyn Database, input: MyInput) -> MyTracked<'_> {
    let t0 = MyTracked::new(db, input, MyList::None);
    MyTracked::new(db, input, MyList::Next(t0))
}

#[test]
fn execute() {
    DatabaseImpl::new().attach(|db| {
        let input = MyInput::new(db, "foo".to_string());
        let t0: MyTracked = create_tracked_list(db, input);
        let t1 = create_tracked_list(db, input);
        expect_test::expect![[r#"
            MyTracked {
                [salsa id]: Id(81),
                data: MyInput {
                    [salsa id]: Id(0),
                    field: "foo",
                },
                next: Next(
                    MyTracked {
                        [salsa id]: Id(80),
                        data: MyInput {
                            [salsa id]: Id(0),
                            field: "foo",
                        },
                        next: None,
                    },
                ),
            }
        "#]]
        .assert_debug_eq(&t0);
        assert_eq!(t0, t1);
    })
}
