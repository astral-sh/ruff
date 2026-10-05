#![allow(dead_code)]

use std::marker::PhantomData;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Foreign<T>(T);
struct ForeignMarker<T>(PhantomData<fn() -> T>);

#[derive(salsa::SalsaValue)]
struct UnconditionallySafe<T> {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    value: ForeignMarker<T>,
}

#[derive(salsa::SalsaValue)]
enum ConditionalEnum<T> {
    Empty,
    Value(#[salsa_value(unsafe(prove(T: salsa::SalsaValue)))] Foreign<T>),
}

#[derive(salsa::SalsaValue)]
struct ExistingWhereClause<T>
where
    T: Send,
{
    #[salsa_value(unsafe(prove(T: salsa::SalsaValue)))]
    value: Foreign<T>,
}

#[derive(salsa::SalsaValue)]
struct ContainsPhantomRef<'db>(PhantomData<&'db ()>);

#[derive(salsa::SalsaValue)]
struct LifetimePredicate<'db> {
    #[salsa_value(unsafe(prove(
        ContainsPhantomRef<'db>: salsa::SalsaValue,
        Self: Sized,
    )))]
    value: Foreign<ContainsPhantomRef<'db>>,
}

trait Family {
    type Value;
}

struct StringFamily;

impl Family for StringFamily {
    type Value = String;
}

#[derive(salsa::SalsaValue)]
struct AssociatedTypePredicate<F: Family> {
    #[salsa_value(unsafe(prove(F::Value: salsa::SalsaValue, Self: Sized)))]
    value: Foreign<F::Value>,
}

#[salsa::interned]
struct InternedValue<'db> {
    text: String,
}

#[salsa::interned]
struct ConditionalInterned<'db> {
    #[salsa_value(unsafe(prove(
        InternedValue<'db>: salsa::SalsaValue,
        Self: Sized,
    )))]
    value: Foreign<InternedValue<'db>>,
}

#[salsa::tracked]
struct ConditionalTracked<'db> {
    #[salsa_value(unsafe(prove(
        InternedValue<'db>: salsa::SalsaValue,
        Self: Sized,
    )))]
    value: Foreign<InternedValue<'db>>,
}

fn assert_salsa_value<T: salsa::SalsaValue>() {}

fn assert_non_static_proofs<'db>()
where
    UnconditionallySafe<&'db ()>: salsa::SalsaValue,
    ConditionalEnum<ContainsPhantomRef<'db>>: salsa::SalsaValue,
    LifetimePredicate<'db>: salsa::SalsaValue,
{
}

fn main() {
    assert_non_static_proofs();
    assert_salsa_value::<ExistingWhereClause<String>>();
    assert_salsa_value::<AssociatedTypePredicate<StringFamily>>();
}
