#![cfg(feature = "inventory")]

//! Test that the `constructor` macro overrides
//! the `new` method's name and `get` and `set`
//! change the name of the getter and setter of the fields.
#![allow(warnings)]

use std::fmt::Display;

use salsa::Setter;

#[salsa::db]
trait Db: salsa::Database {}

#[salsa::input(constructor = from_string)]
struct MyInput {
    #[get(text)]
    #[set(set_text)]
    field: String,
}

impl MyInput {
    pub fn new(db: &mut dyn Db, s: impl Display) -> MyInput {
        MyInput::from_string(db, s.to_string())
    }

    pub fn field(self, db: &dyn Db) -> String {
        self.text(db).clone()
    }

    pub fn set_field(self, db: &mut dyn Db, id: String) {
        self.set_text(db).to(id);
    }
}

#[salsa::interned(constructor = from_string)]
struct MyInterned<'db> {
    #[get(text)]
    #[returns(ref)]
    field: String,
}

impl<'db> MyInterned<'db> {
    pub fn new(db: &'db dyn Db, s: impl Display) -> MyInterned<'db> {
        MyInterned::from_string(db, s.to_string())
    }

    pub fn field(self, db: &'db dyn Db) -> &str {
        &self.text(db)
    }
}

#[salsa::tracked(constructor = from_string)]
struct MyTracked<'db> {
    #[get(text)]
    field: String,
}

impl<'db> MyTracked<'db> {
    pub fn new(db: &'db dyn Db, s: impl Display) -> MyTracked<'db> {
        MyTracked::from_string(db, s.to_string())
    }

    pub fn field(self, db: &'db dyn Db) -> String {
        self.text(db).clone()
    }
}

#[test]
fn execute() {
    salsa::DatabaseImpl::new();
}
