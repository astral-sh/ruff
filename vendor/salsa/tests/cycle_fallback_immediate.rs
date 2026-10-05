#![cfg(feature = "inventory")]

//! It is possible to omit the `cycle_fn`, only specifying `cycle_result` in which case
//! an immediate fallback value is used as the cycle handling opposed to doing a fixpoint resolution.

use salsa::Setter as _;

#[salsa::tracked(returns(copy), cycle_result=cycle_result)]
fn one_o_one(db: &dyn salsa::Database) -> u32 {
    let val = one_o_one(db);
    val + 1
}

fn cycle_result(_db: &dyn salsa::Database, _id: salsa::Id) -> u32 {
    100
}

#[test_log::test]
fn simple() {
    let db = salsa::DatabaseImpl::default();

    assert_eq!(one_o_one(&db), 100);
}

#[salsa::tracked(returns(copy), cycle_result=two_queries_cycle_result)]
fn two_queries1(db: &dyn salsa::Database) -> i32 {
    two_queries2(db) + 1
}

#[salsa::tracked(returns(copy), cycle_result=two_queries_cycle_result)]
fn two_queries2(db: &dyn salsa::Database) -> i32 {
    two_queries1(db)
}

fn two_queries_cycle_result(_db: &dyn salsa::Database, _id: salsa::Id) -> i32 {
    1
}

#[test]
fn two_queries() {
    let db = salsa::DatabaseImpl::default();

    assert_eq!(two_queries1(&db), 1);
    assert_eq!(two_queries2(&db), 1);
}

#[salsa::input]
struct FallbackInput {
    first: i32,
    second: i32,
}

#[salsa::tracked(returns(copy))]
fn fallback_dependency(db: &dyn salsa::Database, input: FallbackInput, second: bool) -> i32 {
    if second {
        *input.second(db)
    } else {
        *input.first(db)
    }
}

#[salsa::tracked(returns(copy), cycle_result = |db, _, input| fallback_dependency(db, input, false))]
fn first_with_dependency(db: &dyn salsa::Database, input: FallbackInput) -> i32 {
    second_with_dependency(db, input) + 1
}

#[salsa::tracked(returns(copy), cycle_result = |db, _, input| fallback_dependency(db, input, true))]
fn second_with_dependency(db: &dyn salsa::Database, input: FallbackInput) -> i32 {
    first_with_dependency(db, input) + 1
}

#[salsa::tracked(returns(copy))]
fn fallback_entry(db: &dyn salsa::Database, input: FallbackInput) -> (i32, i32) {
    (
        first_with_dependency(db, input),
        second_with_dependency(db, input),
    )
}

#[test]
fn head_and_participant_keep_distinct_fallback_dependencies() {
    let mut db = salsa::DatabaseImpl::default();
    let input = FallbackInput::new(&db, 10, 20);
    assert_eq!(fallback_entry(&db, input), (10, 20));

    input.set_second(&mut db).to(30);
    assert_eq!(fallback_entry(&db, input), (10, 30));

    input.set_first(&mut db).to(40);
    assert_eq!(second_with_dependency(&db, input), 30);
    assert_eq!(fallback_entry(&db, input), (40, 30));
}
