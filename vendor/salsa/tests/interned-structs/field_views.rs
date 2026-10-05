use std::cell::Cell;
use std::ops::Deref;

use salsa::{Database, FieldReads};

#[expect(non_camel_case_types)]
mod model {
    #[salsa::interned(field_view = read_fields)]
    pub struct Text<'db> {
        #[returns(deref)]
        pub text: String,
    }

    #[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, field_view = read_fields)]
    pub struct ImmortalText {
        #[returns(deref)]
        pub(super) text: String,
    }

    #[salsa::interned]
    #[expect(non_camel_case_types)]
    pub struct r#type<'db> {
        #[returns(deref)]
        pub text: String,
    }

    #[salsa::interned(field_view = read_fields)]
    #[expect(non_camel_case_types)]
    pub struct r#match<'db> {
        #[returns(deref)]
        pub text: String,
    }
}

fn normal_text<'db>(db: &'db dyn Database, value: model::Text<'db>) -> &'db String {
    let fields = FieldReads::new(db);
    let view = value.read_fields(fields);
    view.text()
}

fn immortal_text<'db>(db: &'db dyn Database, value: model::ImmortalText) -> &'db String {
    let fields = FieldReads::new(db);
    let view = value.read_fields(fields);
    view.text()
}

#[test]
fn raw_string_references_outlive_normal_and_immortal_views() {
    let db = salsa::DatabaseImpl::new();
    let normal = model::Text::new(&db, String::from("normal"));
    let immortal = model::ImmortalText::new(&db, String::from("immortal"));

    let normal_ordinary: &str = normal.text(&db);
    let immortal_ordinary: &str = immortal.text(&db);
    let normal_raw: &String = normal_text(&db, normal);
    let immortal_raw: &String = immortal_text(&db, immortal);

    assert_eq!(normal_raw, "normal");
    assert_eq!(immortal_raw, "immortal");
    assert!(std::ptr::eq(normal_raw.as_str(), normal_ordinary));
    assert!(std::ptr::eq(immortal_raw.as_str(), immortal_ordinary));
    assert!(std::ptr::eq(
        normal_raw,
        normal.read_fields(FieldReads::new(&db)).text(),
    ));
    assert!(std::ptr::eq(
        immortal_raw,
        immortal.read_fields(FieldReads::new(&db)).text(),
    ));
}

#[test]
fn raw_struct_names_preserve_ordinary_and_opted_in_access() {
    let db = salsa::DatabaseImpl::new();
    let ordinary = model::r#type::new(&db, String::from("ordinary"));
    let opted = model::r#match::new(&db, String::from("opted"));

    let ordinary_text: &str = ordinary.text(&db);
    let opted_text: &str = opted.text(&db);
    let raw: &String = opted.read_fields(FieldReads::new(&db)).text();
    assert_eq!(ordinary_text, "ordinary");
    assert_eq!(opted_text, "opted");
    assert_eq!(raw, "opted");
    assert!(std::ptr::eq(raw.as_str(), opted_text));
}

thread_local! {
    static CALLBACKS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

#[derive(Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
struct Counted(u32);

impl Clone for Counted {
    fn clone(&self) -> Self {
        CALLBACKS.with(|counts| {
            let (clones, derefs) = counts.get();
            counts.set((clones + 1, derefs));
        });
        Self(self.0)
    }
}

impl Deref for Counted {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        CALLBACKS.with(|counts| {
            let (clones, derefs) = counts.get();
            counts.set((clones, derefs + 1));
        });
        &self.0
    }
}

#[salsa::interned(constructor = make, field_view = raw_fields)]
struct CallbackFields<'db> {
    #[get(cloned)]
    #[returns(clone)]
    owned: Counted,
    #[get(dereferenced)]
    #[returns(deref)]
    borrowed: Counted,
    #[returns(copy)]
    read_fields: u32,
}

#[test]
fn raw_views_skip_configured_clone_and_deref() {
    let db = salsa::DatabaseImpl::new();
    let value = CallbackFields::make(&db, Counted(17), Counted(29), 41u32);
    let before = CALLBACKS.get();
    let fields = FieldReads::new(&db);
    let view = value.raw_fields(fields);
    assert_eq!(CALLBACKS.get(), before);

    let clone_raw: &Counted = view.cloned();
    let deref_raw: &Counted = view.dereferenced();
    let copy_raw: &u32 = view.read_fields();
    assert_eq!((clone_raw.0, deref_raw.0, *copy_raw), (17, 29, 41));
    assert!(std::ptr::eq(clone_raw, value.raw_fields(fields).cloned()));
    assert!(std::ptr::eq(
        deref_raw,
        value.raw_fields(fields).dereferenced()
    ));
    assert_eq!(CALLBACKS.get(), before);

    let cloned: Counted = value.cloned(&db);
    assert_eq!(cloned.0, 17);
    assert_eq!(CALLBACKS.get(), (before.0 + 1, before.1));
    let dereferenced: &u32 = value.dereferenced(&db);
    assert_eq!(*dereferenced, 29);
    assert!(std::ptr::eq(dereferenced, &deref_raw.0));
    assert_eq!(value.read_fields(&db), 41);
    let after = (before.0 + 1, before.1 + 1);
    assert_eq!(CALLBACKS.get(), after);

    let _: &Counted = value.raw_fields(fields).cloned();
    let _: &Counted = value.raw_fields(fields).dereferenced();
    assert_eq!(CALLBACKS.get(), after);
}
