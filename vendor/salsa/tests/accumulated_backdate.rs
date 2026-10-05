#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Tests that accumulated values are correctly accounted for
//! when backdating a value.

mod common;
use common::LogDatabase;
use expect_test::expect;
use salsa::{Accumulator, Setter};
use test_log::test;

#[salsa::input(debug)]
struct File {
    content: String,
}

#[salsa::accumulator]
#[derive(Debug)]
struct Log(#[allow(dead_code)] String);

#[salsa::tracked(returns(copy))]
fn compile(db: &dyn LogDatabase, input: File) -> u32 {
    parse(db, input)
}

#[salsa::tracked(returns(copy))]
fn parse(db: &dyn LogDatabase, input: File) -> u32 {
    let value: Result<u32, _> = input.content(db).parse();

    match value {
        Ok(value) => value,
        Err(error) => {
            Log(error.to_string()).accumulate(db);
            0
        }
    }
}

#[test]
fn backdate() {
    let mut db = common::LoggerDatabase::default();

    let input = File::new(&db, "0".to_string());

    let logs = compile::accumulated::<Log>(&db, input);
    expect![[r#"[]"#]].assert_eq(&format!("{logs:#?}"));

    input.set_content(&mut db).to("a".to_string());
    let logs = compile::accumulated::<Log>(&db, input);

    expect![[r#"
        [
            Log(
                "invalid digit found in string",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));
}

#[test]
fn backdate_no_diagnostics() {
    let mut db = common::LoggerDatabase::default();

    let input = File::new(&db, "a".to_string());

    let logs = compile::accumulated::<Log>(&db, input);
    expect![[r#"
        [
            Log(
                "invalid digit found in string",
            ),
        ]"#]]
    .assert_eq(&format!("{logs:#?}"));

    input.set_content(&mut db).to("0".to_string());
    let logs = compile::accumulated::<Log>(&db, input);

    expect![[r#"[]"#]].assert_eq(&format!("{logs:#?}"));
}
