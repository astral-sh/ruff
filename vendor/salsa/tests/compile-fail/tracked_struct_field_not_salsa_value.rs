use std::marker::PhantomData;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct NotSalsaValue<'db>(PhantomData<fn() -> &'db ()>);

#[salsa::tracked]
struct MyTracked<'db> {
    field: NotSalsaValue<'db>,
}

fn main() {}
