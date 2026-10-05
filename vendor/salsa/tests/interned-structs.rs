#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.

#[path = "interned-structs/field_views.rs"]
mod field_views;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use expect_test::expect;
use salsa::plumbing::{AsId, FromId};
use test_log::test;

#[salsa::interned(debug)]
struct InternedBoxed<'db> {
    data: Box<str>,
}

#[salsa::interned(debug)]
struct InternedString<'db> {
    data: String,
}

#[salsa::interned(debug)]
struct InternedPair<'db> {
    #[returns(copy)]
    data: (InternedString<'db>, InternedString<'db>),
}

#[salsa::interned(debug)]
struct InternedTwoFields<'db> {
    data1: String,
    data2: String,
}

#[salsa::interned(debug)]
struct InternedVec<'db> {
    data1: Vec<String>,
}

#[salsa::interned(debug)]
struct InternedBoxedSlice<'db> {
    data: Box<[String]>,
}

#[salsa::interned(debug)]
struct InternedPathBuf<'db> {
    data1: PathBuf,
}

#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
struct InternedStringNoLifetime {
    data: String,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, salsa::SalsaValue)]
struct Foo;

#[salsa::interned(debug)]
struct InternedFoo<'db> {
    data: Foo,
}

#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SalsaIdWrapper(salsa::Id);

impl AsId for SalsaIdWrapper {
    fn as_id(&self) -> salsa::Id {
        self.0
    }
}

impl FromId for SalsaIdWrapper {
    fn from_id(id: salsa::Id) -> Self {
        SalsaIdWrapper(id)
    }
}

#[salsa::interned(id = SalsaIdWrapper, debug)]
struct InternedStringWithCustomId<'db> {
    data: String,
}

#[salsa::interned(
    id = SalsaIdWrapper,
    unsafe(no_lifetime),
    revisions = usize::MAX,
    debug
)]
struct InternedStringWithCustomIdAndNoLifetime<'db> {
    data: String,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct Generic<T>(T);

#[salsa::interned(debug)]
struct InternedOverGeneric {
    value: Generic<String>,
}

#[salsa::tracked(returns(clone))]
fn intern_stuff(db: &dyn salsa::Database) -> String {
    let s1 = InternedString::new(db, "Hello, ".to_string());
    let s2 = InternedString::new(db, "World, ");
    let s3 = InternedPair::new(db, (s1, s2));

    format!("{s3:?}")
}

#[test]
fn execute() {
    let db = salsa::DatabaseImpl::new();
    expect![[r#"
        "InternedPair { data: (InternedString { data: \"Hello, \" }, InternedString { data: \"World, \" }) }"
    "#]].assert_debug_eq(&intern_stuff(&db));
}

#[test]
fn interning_returns_equal_keys_for_equal_data() {
    let db = salsa::DatabaseImpl::new();
    let s1 = InternedString::new(&db, "Hello, ".to_string());
    let s2 = InternedString::new(&db, "World, ".to_string());
    let s1_2 = InternedString::new(&db, "Hello, ");
    let s2_2 = InternedString::new(&db, "World, ");
    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
}
#[test]
fn interning_returns_equal_keys_for_equal_data_multi_field() {
    let db = salsa::DatabaseImpl::new();
    let s1 = InternedTwoFields::new(&db, "Hello, ".to_string(), "World");
    let s2 = InternedTwoFields::new(&db, "World, ", "Hello".to_string());
    let s1_2 = InternedTwoFields::new(&db, "Hello, ", "World");
    let s2_2 = InternedTwoFields::new(&db, "World, ", "Hello");
    let new = InternedTwoFields::new(&db, "Hello, World", "");

    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
    assert_ne!(s1, s2_2);
    assert_ne!(s1, new);
}

#[test]
fn interning_boxed() {
    let db = salsa::DatabaseImpl::new();

    assert_eq!(
        InternedBoxed::new(&db, "Hello"),
        InternedBoxed::new(&db, Box::from("Hello"))
    );
}

#[test]
fn interned_structs_have_public_ingredients() {
    use salsa::plumbing::{AsId, ZalsaDatabase};

    let db = salsa::DatabaseImpl::new();
    let s = InternedString::new(&db, String::from("Hello, world!"));
    let underlying_id = s.0;

    let data = InternedString::ingredient(db.zalsa()).data(db.zalsa(), underlying_id.as_id());
    assert_eq!(data.0, "Hello, world!");
}

#[test]
fn interning_vec() {
    let db = salsa::DatabaseImpl::new();
    let s1 = InternedVec::new(&db, ["Hello, ".to_string(), "World".to_string()].as_slice());
    let s2 = InternedVec::new(&db, ["Hello, ", "World"].as_slice());
    let s3 = InternedVec::new(&db, vec!["Hello, ".to_string(), "World".to_string()]);
    let s4 = InternedVec::new(&db, ["Hello, ", "World"].as_slice());
    let s5 = InternedVec::new(&db, ["Hello, ", "World", "Test"].as_slice());
    let s6 = InternedVec::new(&db, ["Hello, ", "World", ""].as_slice());
    let s7 = InternedVec::new(&db, ["Hello, "].as_slice());
    assert_eq!(s1, s2);
    assert_eq!(s1, s3);
    assert_eq!(s1, s4);
    assert_ne!(s1, s5);
    assert_ne!(s1, s6);
    assert_ne!(s5, s6);
    assert_ne!(s6, s7);
}

#[test]
fn interning_path_buf() {
    let db = salsa::DatabaseImpl::new();
    let s1 = InternedPathBuf::new(&db, PathBuf::from("test_path".to_string()));
    let s2 = InternedPathBuf::new(&db, Path::new("test_path"));
    let s3 = InternedPathBuf::new(&db, Path::new("test_path/"));
    let s4 = InternedPathBuf::new(&db, Path::new("test_path/a"));
    assert_eq!(s1, s2);
    assert_eq!(s1, s3);
    assert_ne!(s1, s4);
}

#[test]
fn interning_without_lifetimes() {
    let db = salsa::DatabaseImpl::new();

    let s1 = InternedStringNoLifetime::new(&db, "Hello, ".to_string());
    let s2 = InternedStringNoLifetime::new(&db, "World, ".to_string());
    let s1_2 = InternedStringNoLifetime::new(&db, "Hello, ");
    let s2_2 = InternedStringNoLifetime::new(&db, "World, ");
    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
}

#[test]
fn interning_with_custom_ids() {
    let db = salsa::DatabaseImpl::new();

    let s1 = InternedStringWithCustomId::new(&db, "Hello, ".to_string());
    let s2 = InternedStringWithCustomId::new(&db, "World, ".to_string());
    let s1_2 = InternedStringWithCustomId::new(&db, "Hello, ");
    let s2_2 = InternedStringWithCustomId::new(&db, "World, ");
    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
}

#[test]
fn interning_with_custom_ids_and_no_lifetime() {
    let db = salsa::DatabaseImpl::new();

    let s1 = InternedStringWithCustomIdAndNoLifetime::new(&db, "Hello, ".to_string());
    let s2 = InternedStringWithCustomIdAndNoLifetime::new(&db, "World, ".to_string());
    let s1_2 = InternedStringWithCustomIdAndNoLifetime::new(&db, "Hello, ");
    let s2_2 = InternedStringWithCustomIdAndNoLifetime::new(&db, "World, ");
    assert_eq!(s1, s1_2);
    assert_eq!(s2, s2_2);
}

#[test]
fn interning_reference() {
    let db = salsa::DatabaseImpl::new();

    let s1 = InternedFoo::new(&db, Foo);
    let s2 = InternedFoo::new(&db, &Foo);
    assert_eq!(s1, s2);
}

#[test]
fn interned_generic() {
    let db = salsa::DatabaseImpl::new();

    let s1 = InternedOverGeneric::new(&db, Generic("test".to_string()));
    let s2 = InternedOverGeneric::new(&db, Generic("test".to_string()));
    assert_eq!(s1, s2);
}

#[test]
fn interning_boxed_slice_with_cow() {
    let db = salsa::DatabaseImpl::new();

    // Create an interned boxed slice using a boxed slice directly.
    let boxed: Box<[String]> = vec!["Hello".to_string(), "World".to_string()].into();
    let s1 = InternedBoxedSlice::new(&db, boxed);

    // Looking up with a Cow::Borrowed should find the same interned value.
    let slice = ["Hello".to_string(), "World".to_string()];
    let borrowed: Cow<'_, [String]> = Cow::Borrowed(&slice);
    let s2 = InternedBoxedSlice::new(&db, borrowed);
    assert_eq!(s1, s2);

    // Looking up with a Cow::Owned should also work and reuse the owned value.
    let owned: Cow<'_, [String]> = Cow::Owned(vec!["Hello".to_string(), "World".to_string()]);
    let s3 = InternedBoxedSlice::new(&db, owned);
    assert_eq!(s1, s3);

    // Different values should result in different interned structs.
    let different_slice = ["Different".to_string()];
    let different: Cow<'_, [String]> = Cow::Borrowed(&different_slice);
    let s4 = InternedBoxedSlice::new(&db, different);
    assert_ne!(s1, s4);
}

#[test]
fn interning_string_with_cow() {
    let db = salsa::DatabaseImpl::new();

    // Create an interned string using a String directly.
    let s1 = InternedString::new(&db, "Hello".to_string());

    // Looking up with a Cow::Borrowed should find the same interned value.
    let borrowed: Cow<'_, str> = Cow::Borrowed("Hello");
    let s2 = InternedString::new(&db, borrowed);
    assert_eq!(s1, s2);

    // Looking up with a Cow::Owned should also work.
    let owned: Cow<'_, str> = Cow::Owned("Hello".to_string());
    let s3 = InternedString::new(&db, owned);
    assert_eq!(s1, s3);

    // Different values should result in different interned structs.
    let different: Cow<'_, str> = Cow::Borrowed("Different");
    let s4 = InternedString::new(&db, different);
    assert_ne!(s1, s4);
}

#[test]
fn interning_pathbuf_with_cow() {
    let db = salsa::DatabaseImpl::new();

    // Create an interned path using a PathBuf directly.
    let s1 = InternedPathBuf::new(&db, PathBuf::from("test_path"));

    // Looking up with a Cow::Borrowed should find the same interned value.
    let borrowed: Cow<'_, Path> = Cow::Borrowed(Path::new("test_path"));
    let s2 = InternedPathBuf::new(&db, borrowed);
    assert_eq!(s1, s2);

    // Looking up with a Cow::Owned should also work.
    let owned: Cow<'_, Path> = Cow::Owned(PathBuf::from("test_path"));
    let s3 = InternedPathBuf::new(&db, owned);
    assert_eq!(s1, s3);

    // Different values should result in different interned structs.
    let different: Cow<'_, Path> = Cow::Borrowed(Path::new("different_path"));
    let s4 = InternedPathBuf::new(&db, different);
    assert_ne!(s1, s4);
}

#[test]
fn interning_vec_with_cow() {
    let db = salsa::DatabaseImpl::new();

    // Create an interned vec using a Vec directly.
    let s1 = InternedVec::new(&db, vec!["Hello".to_string(), "World".to_string()]);

    // Looking up with a Cow::Borrowed should find the same interned value.
    let slice = ["Hello".to_string(), "World".to_string()];
    let borrowed: Cow<'_, [String]> = Cow::Borrowed(&slice);
    let s2 = InternedVec::new(&db, borrowed);
    assert_eq!(s1, s2);

    // Looking up with a Cow::Owned should also work.
    let owned: Cow<'_, [String]> = Cow::Owned(vec!["Hello".to_string(), "World".to_string()]);
    let s3 = InternedVec::new(&db, owned);
    assert_eq!(s1, s3);

    // Different values should result in different interned structs.
    let different: Cow<'_, [String]> = Cow::Owned(vec!["Different".to_string()]);
    let s4 = InternedVec::new(&db, different);
    assert_ne!(s1, s4);
}

#[cfg(feature = "compact_str")]
#[salsa::interned(debug)]
struct InternedCompactString<'db> {
    data: compact_str::CompactString,
}

#[cfg(feature = "compact_str")]
#[test]
fn interning_compact_string_with_cow() {
    let db = salsa::DatabaseImpl::new();

    // Create an interned compact string using a CompactString directly.
    let s1 = InternedCompactString::new(&db, compact_str::CompactString::new("Hello"));

    // Looking up with a Cow::Borrowed should find the same interned value.
    let borrowed: Cow<'_, str> = Cow::Borrowed("Hello");
    let s2 = InternedCompactString::new(&db, borrowed);
    assert_eq!(s1, s2);

    // Looking up with a Cow::Owned should also work.
    let owned: Cow<'_, str> = Cow::Owned("Hello".to_string());
    let s3 = InternedCompactString::new(&db, owned);
    assert_eq!(s1, s3);

    // Different values should result in different interned structs.
    let different: Cow<'_, str> = Cow::Borrowed("Different");
    let s4 = InternedCompactString::new(&db, different);
    assert_ne!(s1, s4);
}
