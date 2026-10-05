#![cfg(feature = "inventory")]

use expect_test::expect;
mod common;
use common::LogDatabase;
use salsa::Database;

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(debug)]
struct MyTracked<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("tracked_fn({input:?})"));
    let t = MyTracked::new(db, input.field(db) * 2);
    tracked_fn_extra::specify(db, t, 2222);
    tracked_fn_extra(db, t)
}

#[salsa::tracked(returns(copy))]
fn tracked_fn_specify_twice(db: &dyn LogDatabase, input: MyInput) {
    let t = MyTracked::new(db, input.field(db) * 2);
    tracked_fn_extra::specify(db, t, 1111);
    tracked_fn_extra::specify(db, t, 2222);
}

#[salsa::tracked(returns(copy), specify)]
fn tracked_fn_extra<'db>(db: &'db dyn LogDatabase, input: MyTracked<'db>) -> u32 {
    db.push_log(format!("tracked_fn_extra({input:?})"));
    0
}

#[test]
fn execute() {
    let mut db = common::LoggerDatabase::default();
    let input = MyInput::new(&db, 22);
    assert_eq!(tracked_fn(&db, input), 2222);
    db.assert_logs(expect![[r#"
        [
            "tracked_fn(MyInput { [salsa id]: Id(0), field: 22 })",
        ]"#]]);

    // A "synthetic write" causes the system to act *as though* some
    // input of durability `durability` has changed.
    db.synthetic_write(salsa::Durability::LOW);

    // Re-run the query on the original input. Nothing re-executes!
    assert_eq!(tracked_fn(&db, input), 2222);
    db.assert_logs(expect!["[]"]);
}

#[test]
#[should_panic(expected = "cannot call `specify` twice for the same key in one query execution")]
fn specify_twice_panics() {
    let db = common::LoggerDatabase::default();
    let input = MyInput::new(&db, 22);
    tracked_fn_specify_twice(&db, input);
}
