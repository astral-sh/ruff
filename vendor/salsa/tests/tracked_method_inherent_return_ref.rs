#![cfg(feature = "inventory")]

use salsa::Database;

#[salsa::input]
struct Input {
    #[returns(copy)]
    number: usize,
}

#[salsa::tracked]
impl Input {
    #[salsa::tracked(returns(ref))]
    fn test(self, db: &dyn salsa::Database) -> Vec<String> {
        (0..self.number(db)).map(|i| format!("test {i}")).collect()
    }
}

#[test]
fn invoke() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = Input::new(db, 3);
        let x: &Vec<String> = input.test(db);
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
