use std::marker::PhantomData;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct NotSalsaValue<'db>(PhantomData<fn() -> &'db ()>);

#[salsa::interned(unsafe(non_salsa_values))]
struct InternedWithItemEscape<'db> {
    value: NotSalsaValue<'db>,
}

#[salsa::interned]
struct InternedWithFieldEscape<'db> {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    value: NotSalsaValue<'db>,
}

#[salsa::tracked]
struct TrackedWithFieldEscape<'db> {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    value: NotSalsaValue<'db>,
}

#[salsa::tracked(unsafe(non_salsa_values))]
fn tracked_fn<'db>(
    _db: &'db dyn salsa::Database,
    _value: NotSalsaValue<'db>,
    _other: u32,
) {
}

fn main() {}
