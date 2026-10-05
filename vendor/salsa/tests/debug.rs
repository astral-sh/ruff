#![cfg(feature = "inventory")]

//! Test that `DeriveWithDb` is correctly derived.

use expect_test::expect;
use salsa::{Database, Setter};

#[salsa::input(debug)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct NotSalsa {
    field: String,
}

#[salsa::input(debug)]
struct ComplexStruct {
    #[returns(copy)]
    my_input: MyInput,
    not_salsa: NotSalsa,
}

#[test]
fn input() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = MyInput::new(db, 22);
        let not_salsa = NotSalsa {
            field: "it's salsa time".to_string(),
        };
        let complex_struct = ComplexStruct::new(db, input, not_salsa);

        // debug includes all fields
        let actual = format!("{complex_struct:?}");
        let expected = expect![[r#"ComplexStruct { [salsa id]: Id(80), my_input: MyInput { [salsa id]: Id(0), field: 22 }, not_salsa: NotSalsa { field: "it's salsa time" } }"#]];
        expected.assert_eq(&actual);
    })
}

#[salsa::tracked(returns(clone))]
fn leak_debug_string(_db: &dyn salsa::Database, input: MyInput) -> String {
    format!("{input:?}")
}

/// Test that field reads that occur as part of `Debug` are not tracked.
/// Intentionally leaks the debug string.
/// Don't try this at home, kids.
#[test]
fn untracked_dependencies() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, 22);

    let s = leak_debug_string(&db, input);
    expect![[r#"
        "MyInput { [salsa id]: Id(0), field: 22 }"
    "#]]
    .assert_debug_eq(&s);

    input.set_field(&mut db).to(23);

    // check that we reuse the cached result for debug string
    // even though the dependency changed.
    let s = leak_debug_string(&db, input);
    assert!(s.contains(", field: 22 }"));
}

#[salsa::tracked(returns(copy))]
fn dep_a(db: &dyn salsa::Database, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::tracked(returns(copy))]
fn dep_b(db: &dyn salsa::Database, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::tracked(returns(copy))]
fn debug_branch_query(db: &dyn salsa::Database, selector: MyInput, a: MyInput, b: MyInput) -> u32 {
    // `Debug` for salsa structs is explicitly untracked; branching on it is unsound.
    // This test demonstrates it can break the backdate invariant.
    let s = format!("{selector:?}");
    if s.contains("field: 0") {
        dep_a(db, a)
    } else {
        dep_b(db, b)
    }
}

/// Backdating warns about branching on the output of a Salsa struct's derived `Debug` output,
/// because it doesn't track its reads (can lead to stale results).
#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "returned the same value"))]
fn debug_branch_can_trip_backdate_assertion() {
    let mut db = salsa::DatabaseImpl::new();

    let selector = MyInput::new(&db, 0);
    let a = MyInput::new(&db, 0);
    let b = MyInput::new(&db, 0);

    // R1: depends on `a`, returns 0
    assert_eq!(debug_branch_query(&db, selector, a, b), 0);

    // R2: force `debug_branch_query` to change (0 -> 1) so its memo's changed_at advances.
    a.set_field(&mut db).to(1);
    assert_eq!(debug_branch_query(&db, selector, a, b), 1);

    // R3: change back to 0; memo value is 0 but changed_at is now "recent".
    a.set_field(&mut db).to(0);
    assert_eq!(debug_branch_query(&db, selector, a, b), 0);

    // R4/R5: change `selector` (untracked) so the query switches to `b`, and change `a`
    // to force re-execution. New execution returns 0 (equal) but depends only on older `b`,
    // so `new.changed_at < old.changed_at` and backdating asserts.
    selector.set_field(&mut db).to(1);
    a.set_field(&mut db).to(1);
    let _ = debug_branch_query(&db, selector, a, b);
}

#[salsa::tracked]
struct DerivedCustom<'db> {
    #[returns(copy)]
    my_input: MyInput,
    #[returns(copy)]
    value: u32,
}

impl std::fmt::Debug for DerivedCustom<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        salsa::with_attached_database(|db| {
            write!(f, "{:?} / {:?}", self.my_input(db), self.value(db))
        })
        .unwrap_or_else(|| f.debug_tuple("DerivedCustom").finish())
    }
}

#[salsa::tracked(returns(clone))]
fn leak_derived_custom(db: &dyn salsa::Database, input: MyInput, value: u32) -> String {
    let c = DerivedCustom::new(db, input, value);
    format!("{c:?}")
}

#[test]
fn custom_debug_impl() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, 22);

    let s = leak_derived_custom(&db, input, 23);
    expect![[r#"
        "MyInput { [salsa id]: Id(0), field: 22 } / 23"
    "#]]
    .assert_debug_eq(&s);
}
