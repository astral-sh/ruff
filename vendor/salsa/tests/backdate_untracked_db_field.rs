#![cfg(feature = "inventory")]

use salsa::Setter;

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::db]
trait Db: salsa::Database {
    /// Untracked database state. Reading this inside a tracked query is a footgun.
    fn extra(&self) -> u32;
}

#[salsa::db]
#[derive(Default, Clone)]
struct ExtraFieldDatabase {
    storage: salsa::Storage<Self>,
    extra: u32,
}

#[salsa::db]
impl salsa::Database for ExtraFieldDatabase {}

#[salsa::db]
impl Db for ExtraFieldDatabase {
    fn extra(&self) -> u32 {
        self.extra
    }
}

impl ExtraFieldDatabase {
    fn set_extra(&mut self, value: u32) {
        self.extra = value;
    }
}

#[salsa::tracked(returns(copy))]
fn dep_a_db(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::tracked(returns(copy))]
fn dep_b_db(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::tracked(returns(copy))]
fn db_field_branch_query(db: &dyn Db, a: MyInput, b: MyInput) -> u32 {
    if db.extra() == 0 {
        dep_a_db(db, a)
    } else {
        dep_b_db(db, b)
    }
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "returned the same value"))]
fn db_field_branch_can_trip_backdate_assertion() {
    let mut db = ExtraFieldDatabase::default();
    db.set_extra(0);

    let a = MyInput::new(&db, 0);
    let b = MyInput::new(&db, 0);

    // R1: depends on `a`, returns 0
    assert_eq!(db_field_branch_query(&db, a, b), 0);

    // R2: force the memo to have a recent changed_at.
    a.set_field(&mut db).to(1);
    assert_eq!(db_field_branch_query(&db, a, b), 1);

    // R3: return to 0 (still depending on `a`).
    a.set_field(&mut db).to(0);
    assert_eq!(db_field_branch_query(&db, a, b), 0);

    // R4/R5: switch branch via untracked db field, and force re-execution by changing `a`.
    // New execution returns 0 (equal) but depends only on older `b`, triggering the backdate check.
    db.set_extra(1);
    a.set_field(&mut db).to(1);
    let _ = db_field_branch_query(&db, a, b);
}
