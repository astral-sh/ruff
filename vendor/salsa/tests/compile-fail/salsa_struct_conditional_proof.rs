use std::marker::PhantomData;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Foreign<T>(T);

#[derive(Clone, PartialEq, Eq, Hash)]
struct NotSalsaValue<'db>(PhantomData<&'db ()>);

#[salsa::interned]
struct ConditionalInterned<'db> {
    #[salsa_value(unsafe(prove(NotSalsaValue<'db>: salsa::SalsaValue)))]
    value: Foreign<NotSalsaValue<'db>>,
}

#[salsa::interned(unsafe(non_salsa_values))]
struct ConditionalInternedWithItemEscape<'db> {
    #[salsa_value(unsafe(prove(NotSalsaValue<'db>: salsa::SalsaValue)))]
    value: Foreign<NotSalsaValue<'db>>,
}

#[salsa::tracked]
struct ConditionalTracked<'db> {
    #[salsa_value(unsafe(prove(NotSalsaValue<'db>: salsa::SalsaValue)))]
    value: Foreign<NotSalsaValue<'db>>,
}

fn main() {}
