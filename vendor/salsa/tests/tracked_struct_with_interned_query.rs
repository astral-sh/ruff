#![cfg(feature = "inventory")]

mod common;

use salsa::Setter;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked]
struct Tracked<'db> {
    value: String,
}

#[salsa::tracked(returns(copy))]
fn query_tracked(db: &dyn salsa::Database, input: MyInput) -> Tracked<'_> {
    Tracked::new(db, format!("{value}", value = input.value(db)))
}

#[salsa::tracked(returns(clone))]
fn join<'db>(db: &'db dyn salsa::Database, tracked: Tracked<'db>, with: String) -> String {
    format!("{}{}", tracked.value(db), with)
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::default();
    let input = MyInput::new(&db, 1);

    let tracked = query_tracked(&db, input);
    let joined = join(&db, tracked, "world".to_string());

    assert_eq!(joined, "1world");

    // Create a new revision: This puts the tracked struct created in revision 0
    // into the free list.
    input.set_value(&mut db).to(2);

    let tracked = query_tracked(&db, input);
    let joined = join(&db, tracked, "world".to_string());

    assert_eq!(joined, "2world");

    // Create a new revision: The tracked struct created in revision 0 is now
    // reused, including its id. The argument to `join` will hash and compare
    // equal to the argument used in revision 0 but the return value should be
    // 3world and not 1world.
    input.set_value(&mut db).to(3);

    let tracked = query_tracked(&db, input);
    let joined = join(&db, tracked, "world".to_string());

    assert_eq!(joined, "3world");
}
