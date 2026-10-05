#![cfg(feature = "inventory")]

use salsa::Database;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Item {}

#[salsa::input]
pub struct Input {
    name: String,
}

#[salsa::tracked]
impl Input {
    #[salsa::tracked(returns(copy))]
    pub fn source_tree(self, db: &dyn Database) -> SourceTree<'_> {
        SourceTree::new(db, self.name(db).clone())
    }
}

#[salsa::tracked]
pub struct SourceTree<'db> {
    name: String,
}

#[salsa::tracked]
impl<'db1> SourceTree<'db1> {
    #[salsa::tracked(returns(ref))]
    pub fn inherent_item_name(self, db: &'db1 dyn Database) -> String {
        self.name(db).clone()
    }
}

trait ItemName<'db1> {
    fn trait_item_name(self, db: &'db1 dyn Database) -> &'db1 String;
}

#[salsa::tracked]
impl<'db1> ItemName<'db1> for SourceTree<'db1> {
    #[salsa::tracked(returns(ref))]
    fn trait_item_name(self, db: &'db1 dyn Database) -> String {
        self.name(db).clone()
    }
}

#[test]
fn test_inherent() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = Input::new(db, "foo".to_string());
        let source_tree = input.source_tree(db);
        expect_test::expect![[r#"
            "foo"
        "#]]
        .assert_debug_eq(source_tree.inherent_item_name(db));
    })
}

#[test]
fn test_trait() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = Input::new(db, "foo".to_string());
        let source_tree = input.source_tree(db);
        expect_test::expect![[r#"
            "foo"
        "#]]
        .assert_debug_eq(source_tree.trait_item_name(db));
    })
}
