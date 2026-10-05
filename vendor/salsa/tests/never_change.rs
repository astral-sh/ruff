#![cfg(feature = "inventory")]

mod common;

use common::{DiscardLoggerDatabase, LogDatabase};
#[cfg(not(feature = "persistence"))]
use common::{ExecuteValidateLoggerDatabase, LoggerDatabase};
use expect_test::expect;
use salsa::{Database, Durability, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn immutable_value(db: &dyn Database, input: MyInput) -> u32 {
    input.value(db)
}

#[salsa::tracked(returns(copy))]
fn mixed_value(db: &dyn Database, immutable_input: MyInput, mutable_input: MyInput) -> u32 {
    immutable_value(db, immutable_input) + mutable_input.value(db)
}

#[salsa::tracked(returns(copy))]
fn mixed_input_value(db: &dyn Database, immutable_input: MyInput, mutable_input: MyInput) -> u32 {
    immutable_input.value(db) + mutable_input.value(db)
}

#[salsa::tracked(returns(copy), lru = 1)]
#[cfg(not(feature = "persistence"))]
fn immutable_value_with_lru(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("immutable_value_with_lru({})", input.value(db)));
    input.value(db)
}

#[salsa::tracked(returns(copy))]
#[cfg(not(feature = "persistence"))]
fn value_from_lru(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("value_from_lru({})", input.value(db)));
    immutable_value_with_lru(db, input)
}

#[salsa::tracked]
struct Output<'db> {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn output(db: &dyn Database, input: MyInput) -> Output<'_> {
    let output = Output::new(db, input.value(db));
    specified::specify(db, output, input.value(db) + 1);
    output
}

#[salsa::tracked(returns(copy), lru = 1)]
fn output_with_lru(db: &dyn Database, input: MyInput) -> Output<'_> {
    let output = Output::new(db, input.value(db));
    specified::specify(db, output, input.value(db) + 1);
    output
}

#[salsa::tracked(returns(copy), specify)]
fn specified<'db>(_db: &'db dyn Database, _output: Output<'db>) -> u32 {
    0
}

#[test]
#[cfg(not(feature = "persistence"))]
fn skip_dependency_edge_to_never_change_query() {
    let mut db = ExecuteValidateLoggerDatabase::default();
    let immutable_input = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);
    let mutable_input = MyInput::new(&db, 20);

    assert_eq!(mixed_value(&db, immutable_input, mutable_input), 30);
    db.assert_logs(expect![[r#"
        [
            "salsa_event(WillExecute { database_key: mixed_value(Id(80)) })",
            "salsa_event(WillExecute { database_key: immutable_value(Id(0)) })",
        ]"#]]);

    db.synthetic_write(Durability::LOW);

    assert_eq!(mixed_value(&db, immutable_input, mutable_input), 30);
    db.assert_logs(expect![[r#"
        [
            "salsa_event(DidValidateMemoizedValue { database_key: mixed_value(Id(80)) })",
        ]"#]]);
}

#[test]
#[cfg(all(not(feature = "persistence"), feature = "salsa_unstable"))]
fn skip_dependency_edge_to_never_change_input() {
    let db = salsa::DatabaseImpl::default();
    let immutable_input = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);
    let mutable_input = MyInput::new(&db, 20);

    assert_eq!(mixed_value(&db, immutable_input, mutable_input), 30);
    assert_eq!(mixed_input_value(&db, immutable_input, mutable_input), 30);

    let memory_usage = <dyn Database>::memory_usage(&db);
    assert_eq!(
        memory_usage.queries["mixed_input_value"].size_of_metadata(),
        memory_usage.queries["mixed_value"].size_of_metadata()
    );
}

#[test]
#[should_panic(expected = "never-changing inputs cannot be mutated")]
fn never_change_input_cannot_be_mutated() {
    let mut db = salsa::DatabaseImpl::default();
    let input = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);

    input.set_value(&mut db).to(20);
}

#[test]
#[cfg(not(feature = "persistence"))]
fn callers_can_omit_edges_to_never_change_lru_queries() {
    let mut db = LoggerDatabase::default();
    let input1 = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);
    let input2 = MyInput::builder(20)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);

    assert_eq!(value_from_lru(&db, input1), 10);
    assert_eq!(immutable_value_with_lru(&db, input2), 20);
    db.clear_logs();

    db.synthetic_write(Durability::HIGH);

    assert_eq!(value_from_lru(&db, input1), 10);
    db.assert_logs(expect!["[]"]);

    assert_eq!(immutable_value_with_lru(&db, input1), 10);
    db.assert_logs(expect![[r#"
        [
            "immutable_value_with_lru(10)",
        ]"#]]);
}

#[test]
fn never_change_lru_query_recreates_outputs_after_eviction() {
    let mut db = salsa::DatabaseImpl::default();
    let input1 = MyInput::builder(10)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);
    let input2 = MyInput::builder(20)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);

    {
        let output1 = output_with_lru(&db, input1);
        assert_eq!(output1.value(&db), 10);
        assert_eq!(specified(&db, output1), 11);
    }

    {
        let output2 = output_with_lru(&db, input2);
        assert_eq!(output2.value(&db), 20);
        assert_eq!(specified(&db, output2), 21);
    }

    db.synthetic_write(Durability::HIGH);

    let recreated_output = output_with_lru(&db, input1);
    assert_eq!(recreated_output.value(&db), 10);
    assert_eq!(specified(&db, recreated_output), 11);
}

#[test]
fn becoming_never_change_preserves_recreated_outputs() {
    let mut db = DiscardLoggerDatabase::default();
    let input = MyInput::new(&db, 10);

    let initial_output = output(&db, input);
    assert_eq!(initial_output.value(&db), 10);
    assert_eq!(specified(&db, initial_output), 11);

    input
        .set_value(&mut db)
        .with_durability(Durability::NEVER_CHANGE)
        .to(10);

    let recreated_output = output(&db, input);
    assert_eq!(recreated_output.value(&db), 10);
    assert_eq!(specified(&db, recreated_output), 11);

    db.synthetic_write(Durability::LOW);

    let retained_output = output(&db, input);
    assert_eq!(retained_output.value(&db), 10);
    assert_eq!(specified(&db, retained_output), 11);
    db.assert_logs(expect!["[]"]);
}
