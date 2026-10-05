#![cfg(feature = "inventory")]

mod common;

use common::LogDatabase;
use expect_test::expect;
use salsa::Setter as _;

#[salsa::input]
struct Input {
    #[returns(copy)]
    number: i16,
}

#[salsa::tracked(returns(copy), no_eq)]
fn abs_float(db: &dyn LogDatabase, input: Input) -> f32 {
    let number = input.number(db);

    db.push_log(format!("abs_float({number})"));
    number.abs() as f32
}

#[salsa::tracked(returns(copy))]
fn derived(db: &dyn LogDatabase, input: Input) -> u32 {
    let x = abs_float(db, input);
    db.push_log("derived".to_string());

    x as u32
}
#[test]
fn invoke() {
    let mut db = common::LoggerDatabase::default();

    let input = Input::new(&db, 5);
    let x = derived(&db, input);

    assert_eq!(x, 5);

    input.set_number(&mut db).to(-5);

    // Derived should re-execute even the result of `abs_float` is the same.
    let x = derived(&db, input);
    assert_eq!(x, 5);

    db.assert_logs(expect![[r#"
        [
            "abs_float(5)",
            "derived",
            "abs_float(-5)",
            "derived",
        ]"#]]);
}
