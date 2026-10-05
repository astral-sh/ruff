#![cfg(feature = "inventory")]
// Expected sizes assume a 64-bit target.
#![cfg(target_pointer_width = "64")]

use salsa::{Database as _, Setter as _};

#[salsa::input(heap_size = string_tuple_size_of)]
struct MyInput {
    #[returns(clone)]
    field: String,
}

#[salsa::tracked(heap_size = string_tuple_size_of)]
struct MyTracked<'db> {
    field: String,
}

#[salsa::interned(heap_size = string_tuple_size_of)]
struct MyInterned<'db> {
    field: String,
}

#[salsa::tracked(returns(clone))]
fn input_to_interned(db: &dyn salsa::Database, input: MyInput) -> MyInterned<'_> {
    MyInterned::new(db, input.field(db))
}

#[salsa::tracked(returns(copy))]
fn input_to_tracked(db: &dyn salsa::Database, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, input.field(db))
}

#[salsa::tracked(returns(copy))]
fn maybe_input_to_tracked(db: &dyn salsa::Database, input: MyInput) -> Option<MyTracked<'_>> {
    let field = input.field(db);
    (!field.is_empty()).then(|| MyTracked::new(db, field))
}

#[salsa::tracked(returns(clone))]
fn input_to_string(_db: &dyn salsa::Database) -> String {
    "a".repeat(1000)
}

#[salsa::tracked(returns(clone), heap_size = string_size_of)]
fn input_to_string_get_size(_db: &dyn salsa::Database) -> String {
    "a".repeat(1000)
}

#[salsa::tracked(returns(copy))]
fn input_to_length(db: &dyn salsa::Database, input: MyInput) -> usize {
    input.field(db).len()
}

#[salsa::tracked(returns(copy), cycle_fn = cycle_recover_length, cycle_initial = cycle_initial_length)]
fn cycle_input_to_length(db: &dyn salsa::Database, input: MyInput) -> usize {
    cycle_input_to_length(db, input).max(input.field(db).len())
}

fn cycle_recover_length(
    _db: &dyn salsa::Database,
    _cycle: &salsa::Cycle,
    _last_provisional_value: &usize,
    value: usize,
    _input: MyInput,
) -> usize {
    value
}

fn cycle_initial_length(_db: &dyn salsa::Database, _id: salsa::Id, _input: MyInput) -> usize {
    0
}

fn string_size_of(x: &String) -> usize {
    x.capacity()
}

fn string_tuple_size_of((x,): &(String,)) -> usize {
    x.capacity()
}

#[salsa::tracked(returns(copy))]
fn input_to_tracked_tuple(
    db: &dyn salsa::Database,
    input: MyInput,
) -> (MyTracked<'_>, MyTracked<'_>) {
    (
        MyTracked::new(db, input.field(db)),
        MyTracked::new(db, input.field(db)),
    )
}

#[rustversion::all(stable, since(1.91))]
#[test]
fn test() {
    use expect_test::expect;

    let db = salsa::DatabaseImpl::new();

    let input1 = MyInput::new(&db, "a".repeat(50));
    let input2 = MyInput::new(&db, "a".repeat(150));
    let input3 = MyInput::new(&db, "a".repeat(250));

    let _tracked1 = input_to_tracked(&db, input1);
    let _tracked2 = input_to_tracked(&db, input2);

    let _tracked_tuple = input_to_tracked_tuple(&db, input1);

    let _interned1 = input_to_interned(&db, input1);
    let _interned2 = input_to_interned(&db, input2);
    let _interned3 = input_to_interned(&db, input3);

    let _string1 = input_to_string(&db);
    let _string2 = input_to_string_get_size(&db);

    let memory_usage = <dyn salsa::Database>::memory_usage(&db);

    let input_info = memory_usage
        .structs
        .iter()
        .find(|ingredient| ingredient.debug_name() == "MyInput")
        .unwrap();
    let input_pages = input_info.page_info().unwrap();
    assert_eq!(input_pages.page_count(), 1);
    assert_eq!(input_pages.page_capacity(), 128);
    assert_eq!(input_pages.excess_capacity(), 125);
    assert_eq!(input_pages.p25_fill(), 3);
    assert_eq!(input_pages.p50_fill(), 3);
    assert_eq!(input_pages.p75_fill(), 3);
    assert_eq!(input_pages.p90_fill(), 3);
    assert_eq!(input_pages.p99_fill(), 3);
    assert!(
        memory_usage
            .queries
            .values()
            .all(|query| query.page_info().is_none())
    );

    let expected = expect![[r#"
        [
            IngredientInfo {
                debug_name: "MyInput",
                count: 3,
                size_of_metadata: 96,
                size_of_fields: 72,
                heap_size_of_fields: Some(
                    450,
                ),
                page_info: Some(
                    PageInfo {
                        page_count: 1,
                        page_capacity: 128,
                        excess_capacity: 125,
                        p25_fill: 3,
                        p50_fill: 3,
                        p75_fill: 3,
                        p90_fill: 3,
                        p99_fill: 3,
                    },
                ),
            },
            IngredientInfo {
                debug_name: "MyInterned",
                count: 3,
                size_of_metadata: 168,
                size_of_fields: 72,
                heap_size_of_fields: Some(
                    450,
                ),
                page_info: Some(
                    PageInfo {
                        page_count: 1,
                        page_capacity: 128,
                        excess_capacity: 125,
                        p25_fill: 3,
                        p50_fill: 3,
                        p75_fill: 3,
                        p90_fill: 3,
                        p99_fill: 3,
                    },
                ),
            },
            IngredientInfo {
                debug_name: "MyTracked",
                count: 4,
                size_of_metadata: 128,
                size_of_fields: 96,
                heap_size_of_fields: Some(
                    300,
                ),
                page_info: Some(
                    PageInfo {
                        page_count: 1,
                        page_capacity: 128,
                        excess_capacity: 124,
                        p25_fill: 4,
                        p50_fill: 4,
                        p75_fill: 4,
                        p90_fill: 4,
                        p99_fill: 4,
                    },
                ),
            },
            IngredientInfo {
                debug_name: "input_to_string::interned_arguments",
                count: 1,
                size_of_metadata: 56,
                size_of_fields: 0,
                heap_size_of_fields: None,
                page_info: Some(
                    PageInfo {
                        page_count: 1,
                        page_capacity: 128,
                        excess_capacity: 127,
                        p25_fill: 1,
                        p50_fill: 1,
                        p75_fill: 1,
                        p90_fill: 1,
                        p99_fill: 1,
                    },
                ),
            },
            IngredientInfo {
                debug_name: "input_to_string_get_size::interned_arguments",
                count: 1,
                size_of_metadata: 56,
                size_of_fields: 0,
                heap_size_of_fields: None,
                page_info: Some(
                    PageInfo {
                        page_count: 1,
                        page_capacity: 128,
                        excess_capacity: 127,
                        p25_fill: 1,
                        p50_fill: 1,
                        p75_fill: 1,
                        p90_fill: 1,
                        p99_fill: 1,
                    },
                ),
            },
        ]"#]];

    expected.assert_eq(&format!("{:#?}", memory_usage.structs));

    let mut queries_info = memory_usage.queries.into_iter().collect::<Vec<_>>();
    queries_info.sort();

    let expected = expect![[r#"
        [
            (
                "input_to_interned",
                IngredientInfo {
                    debug_name: "memory_usage::MyInterned<'_>",
                    count: 3,
                    size_of_metadata: 144,
                    size_of_fields: 24,
                    heap_size_of_fields: None,
                    page_info: None,
                },
            ),
            (
                "input_to_string",
                IngredientInfo {
                    debug_name: "alloc::string::String",
                    count: 1,
                    size_of_metadata: 32,
                    size_of_fields: 24,
                    heap_size_of_fields: None,
                    page_info: None,
                },
            ),
            (
                "input_to_string_get_size",
                IngredientInfo {
                    debug_name: "alloc::string::String",
                    count: 1,
                    size_of_metadata: 32,
                    size_of_fields: 24,
                    heap_size_of_fields: Some(
                        1000,
                    ),
                    page_info: None,
                },
            ),
            (
                "input_to_tracked",
                IngredientInfo {
                    debug_name: "memory_usage::MyTracked<'_>",
                    count: 2,
                    size_of_metadata: 288,
                    size_of_fields: 16,
                    heap_size_of_fields: None,
                    page_info: None,
                },
            ),
            (
                "input_to_tracked_tuple",
                IngredientInfo {
                    debug_name: "(memory_usage::MyTracked<'_>, memory_usage::MyTracked<'_>)",
                    count: 1,
                    size_of_metadata: 168,
                    size_of_fields: 16,
                    heap_size_of_fields: None,
                    page_info: None,
                },
            ),
        ]"#]];

    expected.assert_eq(&format!("{queries_info:#?}"));
}

#[test]
fn cancellation_does_not_allocate_extra_for_ordinary_memos() {
    let mut db = salsa::DatabaseImpl::new();
    let input1 = MyInput::new(&db, "a".repeat(50));
    let input2 = MyInput::new(&db, "a".repeat(150));

    assert_eq!(input_to_length(&db, input1), 50);
    let before = <dyn salsa::Database>::memory_usage(&db);
    let before = &before.queries["input_to_length"];
    assert_eq!(before.count(), 1);

    db.trigger_lru_eviction();

    assert_eq!(input_to_length(&db, input2), 150);
    let after = <dyn salsa::Database>::memory_usage(&db);
    let after = &after.queries["input_to_length"];
    assert_eq!(after.count(), 2);
    assert_eq!(after.size_of_metadata(), before.size_of_metadata() * 2);
}

#[test]
#[cfg(not(feature = "persistence"))]
fn never_change_query_discards_edges() {
    let db = salsa::DatabaseImpl::new();
    let never_change = MyInput::builder("a".repeat(50))
        .durability(salsa::Durability::NEVER_CHANGE)
        .new(&db);
    let mutable = MyInput::new(&db, "a".repeat(150));

    assert_eq!(input_to_length(&db, never_change), 50);
    let before = <dyn salsa::Database>::memory_usage(&db);
    let before = &before.queries["input_to_length"];
    assert_eq!(before.count(), 1);

    assert_eq!(input_to_length(&db, mutable), 150);
    let after = <dyn salsa::Database>::memory_usage(&db);
    let after = &after.queries["input_to_length"];
    assert_eq!(after.count(), 2);
    assert!(after.size_of_metadata() > before.size_of_metadata() * 2);
}

#[test]
#[cfg(not(feature = "persistence"))]
fn never_change_cycle_query_discards_edges_after_converging() {
    let db = salsa::DatabaseImpl::new();
    let never_change = MyInput::builder("a".repeat(50))
        .durability(salsa::Durability::NEVER_CHANGE)
        .new(&db);
    let mutable = MyInput::new(&db, "a".repeat(150));

    assert_eq!(cycle_input_to_length(&db, never_change), 50);
    let before = <dyn salsa::Database>::memory_usage(&db);
    let before = &before.queries["cycle_input_to_length"];
    assert_eq!(before.count(), 1);

    assert_eq!(cycle_input_to_length(&db, mutable), 150);
    let after = <dyn salsa::Database>::memory_usage(&db);
    let after = &after.queries["cycle_input_to_length"];
    assert_eq!(after.count(), 2);
    assert!(after.size_of_metadata() > before.size_of_metadata() * 2);
}

#[test]
fn page_info_tracks_allocated_slots_after_tracked_struct_deletion() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, "value".to_owned());

    assert!(maybe_input_to_tracked(&db, input).is_some());
    input.set_field(&mut db).to(String::new());
    assert!(maybe_input_to_tracked(&db, input).is_none());

    let memory_usage = <dyn salsa::Database>::memory_usage(&db);
    let tracked = memory_usage
        .structs
        .iter()
        .find(|ingredient| ingredient.debug_name() == "MyTracked")
        .unwrap();

    assert_eq!(tracked.count(), 0);

    let pages = tracked.page_info().unwrap();
    assert_eq!(pages.page_count(), 1);
    assert_eq!(pages.excess_capacity(), pages.page_capacity() - 1);
    assert_eq!(pages.p25_fill(), 1);
    assert_eq!(pages.p50_fill(), 1);
    assert_eq!(pages.p75_fill(), 1);
    assert_eq!(pages.p90_fill(), 1);
    assert_eq!(pages.p99_fill(), 1);
}
