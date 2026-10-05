#![cfg(feature = "inventory")]

use salsa::Database;

#[salsa::input]
struct Input {
    #[returns(copy)]
    number: usize,
}

#[salsa::tracked(returns(ref))]
fn test(db: &dyn salsa::Database, input: Input) -> Vec<String> {
    (0..input.number(db)).map(|i| format!("test {i}")).collect()
}

#[test]
fn invoke() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = Input::new(db, 3);
        let x: &Vec<String> = test(db, input);
        expect_test::expect![[r#"
            [
                "test 0",
                "test 1",
                "test 2",
            ]
        "#]]
        .assert_debug_eq(x);
    })
}
