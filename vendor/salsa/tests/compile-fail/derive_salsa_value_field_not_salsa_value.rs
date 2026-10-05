use std::marker::PhantomData;

struct NotSalsaValue<'db>(PhantomData<fn() -> &'db ()>);

#[derive(salsa::SalsaValue)]
struct Value<'db> {
    field: NotSalsaValue<'db>,
}

fn main() {}
