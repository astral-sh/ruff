#![cfg(feature = "inventory")]

//! Delete cascade:
//!
//! * when we delete memoized data, also delete outputs from that data

mod common;
use common::LogDatabase;
use expect_test::expect;
use salsa::Setter;
use test_log::test;

#[salsa::input(singleton, debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn final_result(db: &dyn LogDatabase, input: MyInput) -> u32 {
    db.push_log(format!("final_result({input:?})"));
    let mut sum = 0;
    for tracked_struct in create_tracked_structs(db, input) {
        sum += contribution_from_struct(db, tracked_struct);
    }
    sum
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(clone))]
fn create_tracked_structs(db: &dyn LogDatabase, input: MyInput) -> Vec<MyTracked<'_>> {
    db.push_log(format!("intermediate_result({input:?})"));
    (0..input.field(db))
        .map(|i| MyTracked::new(db, i))
        .collect()
}

#[salsa::tracked(returns(copy))]
fn contribution_from_struct<'db>(db: &'db dyn LogDatabase, tracked: MyTracked<'db>) -> u32 {
    let m = MyTracked::new(db, tracked.field(db));
    copy_field(db, m) * 2
}

#[salsa::tracked(returns(copy))]
fn copy_field<'db>(db: &'db dyn LogDatabase, tracked: MyTracked<'db>) -> u32 {
    tracked.field(db)
}

#[test]
fn basic() {
    let mut db = common::DiscardLoggerDatabase::default();

    // Creates 3 tracked structs
    let input = MyInput::new(&db, 3);
    assert_eq!(final_result(&db, input), 2 * 2 + 2);
    db.assert_logs(expect![[r#"
        [
            "final_result(MyInput { [salsa id]: Id(0), field: 3 })",
            "intermediate_result(MyInput { [salsa id]: Id(0), field: 3 })",
        ]"#]]);

    // Creates only 2 tracked structs in this revision, should delete 1
    //
    // Expect to see 6 DidDiscard events. Three from the primary struct:
    //
    // * the struct itself
    // * the struct's field
    // * the `contribution_from_struct` result
    //
    // and then 3 more from the struct created by `contribution_from_struct`:
    //
    // * the struct itself
    // * the struct's field
    // * the `copy_field` result

    input.set_field(&mut db).to(2);
    assert_eq!(final_result(&db, input), 2);
    db.assert_logs(expect![[r#"
        [
            "intermediate_result(MyInput { [salsa id]: Id(0), field: 2 })",
            "salsa_event(WillDiscardStaleOutput { execute_key: create_tracked_structs(Id(0)), output_key: MyTracked(Id(82)) })",
            "salsa_event(DidDiscard { key: MyTracked(Id(82)) })",
            "salsa_event(DidDiscard { key: contribution_from_struct(Id(82)) })",
            "salsa_event(DidDiscard { key: MyTracked(Id(85)) })",
            "salsa_event(DidDiscard { key: copy_field(Id(85)) })",
            "final_result(MyInput { [salsa id]: Id(0), field: 2 })",
        ]"#]]);
}
