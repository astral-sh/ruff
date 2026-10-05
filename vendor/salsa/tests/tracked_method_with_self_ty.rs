#![cfg(feature = "inventory")]

//! Test that a `tracked` fn with `Self` in its signature or body on a `salsa::input`
//! compiles and executes successfully.
#![allow(warnings)]

trait TrackedTrait {
    type Type;

    fn tracked_trait_fn(self, db: &dyn salsa::Database, ty: Self::Type) -> Self::Type;

    fn untracked_trait_fn();
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
impl MyInput {
    #[salsa::tracked(returns(copy))]
    fn tracked_fn(self, db: &dyn salsa::Database, other: Self) -> u32 {
        self.field(db) + other.field(db)
    }
}

#[salsa::tracked]
impl TrackedTrait for MyInput {
    type Type = u32;

    #[salsa::tracked(returns(copy))]
    fn tracked_trait_fn(self, db: &dyn salsa::Database, ty: Self::Type) -> Self::Type {
        Self::untracked_trait_fn();
        Self::tracked_fn(self, db, self) + ty
    }

    fn untracked_trait_fn() {}
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let object = MyInput::new(&mut db, 10);
    assert_eq!(object.tracked_trait_fn(&db, 1), 21);
}
