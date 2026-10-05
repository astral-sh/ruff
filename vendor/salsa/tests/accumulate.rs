#![cfg(all(feature = "inventory", feature = "accumulator"))]

mod common;
use common::{LogDatabase, LoggerDatabase};
use expect_test::expect;
use salsa::{Accumulator, Setter};
use test_log::test;

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    field_a: u32,
    #[returns(copy)]
    field_b: u32,
}

#[salsa::accumulator]
#[derive(Debug)]
struct Log(#[allow(dead_code)] String);

#[salsa::tracked(returns(copy))]
fn push_logs(db: &dyn LogDatabase, input: MyInput) {
    db.push_log(format!(
        "push_logs(a = {}, b = {})",
        input.field_a(db),
        input.field_b(db)
    ));

    // We don't invoke `push_a_logs` (or `push_b_logs`) with a value of 0.
    // This allows us to test what happens a change in inputs causes a function not to be called at all.
    if input.field_a(db) > 0 {
        push_a_logs(db, input);
    }

    if input.field_b(db) > 0 {
        push_b_logs(db, input);
    }
}

#[salsa::tracked(returns(copy))]
fn push_a_logs(db: &dyn LogDatabase, input: MyInput) {
    let field_a = input.field_a(db);
    db.push_log(format!("push_a_logs({field_a})"));

    for i in 0..field_a {
        Log(format!("log_a({i} of {field_a})")).accumulate(db);
    }
}

#[salsa::tracked(returns(copy))]
fn push_b_logs(db: &dyn LogDatabase, input: MyInput) {
    let field_a = input.field_b(db);
    db.push_log(format!("push_b_logs({field_a})"));

    for i in 0..field_a {
        Log(format!("log_b({i} of {field_a})")).accumulate(db);
    }
}

#[test]
fn accumulate_once() {
    let db = common::LoggerDatabase::default();

    // Just call accumulate on a base input to see what happens.
    let input = MyInput::new(&db, 2, 3);
    let logs = push_logs::accumulated::<Log>(&db, input);
    db.assert_logs(expect![[r#"
        [
            "push_logs(a = 2, b = 3)",
            "push_a_logs(2)",
            "push_b_logs(3)",
        ]"#]]);
    // Check that we see logs from `a` first and then logs from `b`
    // (execution order).
    expect![[r#"
        [
            Log(
                "log_a(0 of 2)",
            ),
            Log(
                "log_a(1 of 2)",
            ),
            Log(
                "log_b(0 of 3)",
            ),
            Log(
                "log_b(1 of 3)",
            ),
            Log(
                "log_b(2 of 3)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
}

#[test]
fn change_a_from_2_to_0() {
    let mut db = common::LoggerDatabase::default();

    // Accumulate logs for `a = 2` and `b = 3`
    let input = MyInput::new(&db, 2, 3);
    let logs = push_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_a(0 of 2)",
            ),
            Log(
                "log_a(1 of 2)",
            ),
            Log(
                "log_b(0 of 3)",
            ),
            Log(
                "log_b(1 of 3)",
            ),
            Log(
                "log_b(2 of 3)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
    db.assert_logs(expect![[r#"
        [
            "push_logs(a = 2, b = 3)",
            "push_a_logs(2)",
            "push_b_logs(3)",
        ]"#]]);

    // Change to `a = 0`, which means `push_logs` does not call `push_a_logs` at all
    input.set_field_a(&mut db).to(0);
    let logs = push_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_b(0 of 3)",
            ),
            Log(
                "log_b(1 of 3)",
            ),
            Log(
                "log_b(2 of 3)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
    db.assert_logs(expect![[r#"
        [
            "push_logs(a = 0, b = 3)",
        ]"#]]);
}

#[test]
fn change_a_from_2_to_1() {
    let mut db = LoggerDatabase::default();

    // Accumulate logs for `a = 2` and `b = 3`
    let input = MyInput::new(&db, 2, 3);
    let logs = push_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_a(0 of 2)",
            ),
            Log(
                "log_a(1 of 2)",
            ),
            Log(
                "log_b(0 of 3)",
            ),
            Log(
                "log_b(1 of 3)",
            ),
            Log(
                "log_b(2 of 3)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
    db.assert_logs(expect![[r#"
        [
            "push_logs(a = 2, b = 3)",
            "push_a_logs(2)",
            "push_b_logs(3)",
        ]"#]]);

    // Change to `a = 1`, which means `push_logs` does not call `push_a_logs` at all
    input.set_field_a(&mut db).to(1);
    let logs = push_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_a(0 of 1)",
            ),
            Log(
                "log_b(0 of 3)",
            ),
            Log(
                "log_b(1 of 3)",
            ),
            Log(
                "log_b(2 of 3)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
    db.assert_logs(expect![[r#"
        [
            "push_logs(a = 1, b = 3)",
            "push_a_logs(1)",
        ]"#]]);
}

#[test]
fn get_a_logs_after_changing_b() {
    let mut db = common::LoggerDatabase::default();

    // Invoke `push_a_logs` with `a = 2` and `b = 3` (but `b` doesn't matter)
    let input = MyInput::new(&db, 2, 3);
    let logs = push_a_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_a(0 of 2)",
            ),
            Log(
                "log_a(1 of 2)",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
    db.assert_logs(expect![[r#"
        [
            "push_a_logs(2)",
        ]"#]]);

    // Changing `b` does not cause `push_a_logs` to re-execute
    // and we still get the same result
    input.set_field_b(&mut db).to(5);
    let logs = push_a_logs::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "log_a(0 of 2)",
            ),
            Log(
                "log_a(1 of 2)",
            ),
        ]
    "#]]
    .assert_debug_eq(&logs);
    db.assert_logs(expect!["[]"]);
}
