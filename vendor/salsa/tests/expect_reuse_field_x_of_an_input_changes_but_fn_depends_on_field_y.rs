#![cfg(feature = "inventory")]

//! Test that if field X of an input changes but not field Y,
//! functions that depend on X re-execute, but those depending only on Y do not
//! compiles and executes successfully.
#![allow(dead_code)]

mod common;
use common::LogDatabase;
use expect_test::expect;
use salsa::Setter;

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    x: u32,
    #[returns(copy)]
    y: u32,
}

#[salsa::tracked(returns(copy))]
fn result_depends_on_x(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("result_depends_on_x({input:?})"));
    input.x(db) + 1
}

#[salsa::tracked(returns(copy))]
fn result_depends_on_y(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("result_depends_on_y({input:?})"));
    input.y(db) - 1
}

#[test]
fn execute() {
    // result_depends_on_x = x + 1
    // result_depends_on_y = y - 1
    let mut db = common::LoggerDatabase::default();

    let input = MyInput::new(&db, 22, 33);
    assert_eq!(result_depends_on_x(&db, input), 23);
    db.assert_logs(expect![[r#"
        [
            "result_depends_on_x(MyInput { [salsa id]: Id(0), x: 22, y: 33 })",
        ]"#]]);

    assert_eq!(result_depends_on_y(&db, input), 32);
    db.assert_logs(expect![[r#"
        [
            "result_depends_on_y(MyInput { [salsa id]: Id(0), x: 22, y: 33 })",
        ]"#]]);

    input.set_x(&mut db).to(23);
    // input x changes, so result depends on x needs to be recomputed;
    assert_eq!(result_depends_on_x(&db, input), 24);
    db.assert_logs(expect![[r#"
        [
            "result_depends_on_x(MyInput { [salsa id]: Id(0), x: 23, y: 33 })",
        ]"#]]);

    // input y is the same, so result depends on y
    // does not need to be recomputed;
    assert_eq!(result_depends_on_y(&db, input), 32);
    db.assert_logs(expect!["[]"]);
}
