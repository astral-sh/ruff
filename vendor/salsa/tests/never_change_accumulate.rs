#![cfg(all(feature = "inventory", feature = "accumulator"))]

use expect_test::expect;
use salsa::{Accumulator, Database, Durability};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    value: u32,
}

#[salsa::accumulator]
#[derive(Debug)]
struct Log(#[allow(dead_code)] u32);

#[salsa::tracked(returns(copy))]
fn push_log(db: &dyn Database, input: MyInput) {
    Log(input.value(db)).accumulate(db);
}

#[salsa::tracked(returns(copy))]
fn outer(db: &dyn Database, input: MyInput) {
    push_log(db, input);
}

#[test]
fn retain_dependency_edge_to_never_change_query_with_accumulated_values() {
    let mut db = salsa::DatabaseImpl::default();
    let input = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);

    expect![[r#"
        [
            Log(
                10,
            ),
        ]
    "#]]
    .assert_debug_eq(&outer::accumulated::<Log>(&db, input));

    db.synthetic_write(Durability::LOW);

    expect![[r#"
        [
            Log(
                10,
            ),
        ]
    "#]]
    .assert_debug_eq(&outer::accumulated::<Log>(&db, input));
}
