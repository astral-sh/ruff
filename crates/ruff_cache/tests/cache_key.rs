use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;

#[derive(CacheKey, Hash)]
struct UnitStruct;

#[derive(CacheKey, Hash)]
struct NamedFieldsStruct {
    a: String,
    b: String,
}

#[derive(CacheKey, Hash)]
struct UnnamedFieldsStruct(String, String);

#[derive(CacheKey, Hash)]
enum Enum {
    Unit,
    UnnamedFields(String, String),
    NamedFields { a: String, b: String },
}

#[test]
fn unit_struct_cache_key() {
    let mut key = CacheKeyHasher::new();

    UnitStruct.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    UnitStruct.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}

#[test]
fn named_field_struct() {
    let mut key = CacheKeyHasher::new();

    let named_fields = NamedFieldsStruct {
        a: "Hello".into(),
        b: "World".into(),
    };

    named_fields.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    named_fields.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}

#[test]
fn unnamed_field_struct() {
    let mut key = CacheKeyHasher::new();

    let unnamed_fields = UnnamedFieldsStruct("Hello".into(), "World".into());

    unnamed_fields.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    unnamed_fields.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}

#[test]
fn enum_unit_variant() {
    let mut key = CacheKeyHasher::new();

    let variant = Enum::Unit;
    variant.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    variant.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}

#[test]
fn enum_named_fields_variant() {
    let mut key = CacheKeyHasher::new();

    let variant = Enum::NamedFields {
        a: "Hello".to_string(),
        b: "World".to_string(),
    };
    variant.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    variant.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}

#[test]
fn enum_unnamed_fields_variant() {
    let mut key = CacheKeyHasher::new();

    let variant = Enum::UnnamedFields("Hello".to_string(), "World".to_string());
    variant.cache_key(&mut key);

    let mut hash = DefaultHasher::new();
    variant.hash(&mut hash);

    assert_eq!(hash.finish(), key.finish());
}
