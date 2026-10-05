//! Tests that overlapping variant types in supertype enums are detected.
#![cfg(feature = "inventory")]

use salsa::plumbing::ZalsaDatabase;

#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
struct Name {
    text: String,
}

#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
struct Age {
    #[returns(copy)]
    value: u32,
}

#[salsa::input(debug)]
struct Input {
    #[returns(copy)]
    data: u32,
}

// ---- Test: direct overlap (same type in two variants) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum DirectOverlap {
    First(Name),
    Second(Name), // same type as First
}

#[test]
#[should_panic(expected = "overlapping variants")]
fn direct_overlap_detected() {
    let db = salsa::DatabaseImpl::new();
    let _name = Name::new(&db, "hello".to_string());
    let _ =
        <DirectOverlap as salsa::plumbing::SalsaStructInDb>::lookup_ingredient_index(db.zalsa());
}

// ---- Test: transitive overlap (nested supertype contains same type) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum Inner {
    Name(Name),
    Age(Age),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum TransitiveOverlap {
    Inner(Inner), // transitively contains Name and Age
    Name(Name),   // overlaps with Inner's Name variant
}

#[test]
#[should_panic(expected = "overlapping variants")]
fn transitive_overlap_detected() {
    let db = salsa::DatabaseImpl::new();
    let _name = Name::new(&db, "hello".to_string());
    let _ = <TransitiveOverlap as salsa::plumbing::SalsaStructInDb>::lookup_ingredient_index(
        db.zalsa(),
    );
}

// ---- Test: no overlap (disjoint types) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum NoOverlap {
    Name(Name),
    Age(Age),
}

#[test]
fn no_overlap_is_fine() {
    let db = salsa::DatabaseImpl::new();
    let _name = Name::new(&db, "hello".to_string());
    // This should NOT panic
    let _ = <NoOverlap as salsa::plumbing::SalsaStructInDb>::lookup_ingredient_index(db.zalsa());
}

// ---- Test: no overlap with nesting (disjoint nested supertypes) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum DisjointInner {
    Name(Name),
    Age(Age),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
enum DisjointOuter {
    Inner(DisjointInner),
    Input(Input), // Input is not in DisjointInner, so no overlap
}

#[test]
fn disjoint_nesting_is_fine() {
    let db = salsa::DatabaseImpl::new();
    let _name = Name::new(&db, "hello".to_string());
    // This should NOT panic
    let _ =
        <DisjointOuter as salsa::plumbing::SalsaStructInDb>::lookup_ingredient_index(db.zalsa());
}
