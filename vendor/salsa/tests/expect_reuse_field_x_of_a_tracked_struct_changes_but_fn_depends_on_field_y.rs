#![cfg(feature = "inventory")]

//! Test that if field X of a tracked struct changes but not field Y,
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
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn final_result_depends_on_x(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("final_result_depends_on_x({input:?})"));
    intermediate_result(db, input).x(db) * 2
}

#[salsa::tracked(returns(copy))]
fn final_result_depends_on_y(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("final_result_depends_on_y({input:?})"));
    intermediate_result(db, input).y(db) * 2
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[tracked]
    #[returns(copy)]
    x: u32,
    #[tracked]
    #[returns(copy)]
    y: u32,
}

#[salsa::tracked(returns(copy))]
fn intermediate_result(db: &dyn LogDatabase, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, input.field(db).div_ceil(2), input.field(db) / 2)
}

#[test]
fn execute() {
    // x = (input.field + 1) / 2
    // y = input.field / 2
    // final_result_depends_on_x = x * 2 = (input.field + 1) / 2 * 2
    // final_result_depends_on_y = y * 2 = input.field / 2 * 2
    let mut db = common::LoggerDatabase::default();

    // intermediate results:
    // x = (22 + 1) / 2 = 11
    // y = 22 / 2 = 11
    let input = MyInput::new(&db, 22);
    assert_eq!(final_result_depends_on_x(&db, input), 22);
    db.assert_logs(expect![[r#"
        [
            "final_result_depends_on_x(MyInput { [salsa id]: Id(0), field: 22 })",
        ]"#]]);

    assert_eq!(final_result_depends_on_y(&db, input), 22);
    db.assert_logs(expect![[r#"
        [
            "final_result_depends_on_y(MyInput { [salsa id]: Id(0), field: 22 })",
        ]"#]]);

    input.set_field(&mut db).to(23);
    // x = (23 + 1) / 2 = 12
    // Intermediate result x changes, so final result depends on x
    // needs to be recomputed;
    assert_eq!(final_result_depends_on_x(&db, input), 24);
    db.assert_logs(expect![[r#"
        [
            "final_result_depends_on_x(MyInput { [salsa id]: Id(0), field: 23 })",
        ]"#]]);

    // y = 23 / 2 = 11
    // Intermediate result y is the same, so final result depends on y
    // does not need to be recomputed;
    assert_eq!(final_result_depends_on_y(&db, input), 22);
    db.assert_logs(expect!["[]"]);
}
