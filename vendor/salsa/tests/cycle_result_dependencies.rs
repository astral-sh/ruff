#![cfg(feature = "inventory")]

use salsa::{Database, Setter};

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: i32,
}

#[salsa::tracked(returns(copy), cycle_result=cycle_result)]
fn has_cycle(db: &dyn Database, input: Input) -> i32 {
    has_cycle(db, input)
}

fn cycle_result(db: &dyn Database, _id: salsa::Id, input: Input) -> i32 {
    input.value(db)
}

#[test]
fn cycle_result_dependencies_are_recorded() {
    let mut db = salsa::DatabaseImpl::default();
    let input = Input::new(&db, 123);
    assert_eq!(has_cycle(&db, input), 123);

    input.set_value(&mut db).to(456);
    assert_eq!(has_cycle(&db, input), 456);
}
