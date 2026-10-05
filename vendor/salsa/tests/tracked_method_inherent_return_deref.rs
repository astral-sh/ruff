#![cfg(feature = "inventory")]

use salsa::Database;

#[salsa::input]
struct Input {
    #[returns(copy)]
    number: usize,
}

#[salsa::tracked]
impl Input {
    #[salsa::tracked(returns(deref))]
    fn test(self, db: &dyn salsa::Database) -> Vec<String> {
        (0..self.number(db)).map(|i| format!("test {i}")).collect()
    }
}

#[test]
fn invoke() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = Input::new(db, 3);
        let x: &[String] = input.test(db);

        assert_eq!(
            x,
            &[
                "test 0".to_string(),
                "test 1".to_string(),
                "test 2".to_string()
            ]
        );
    })
}
