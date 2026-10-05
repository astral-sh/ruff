use std::marker::PhantomData;

use salsa::Database as Db;

mod diagnostic {}

#[salsa::input]
struct MyInput {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotSalsaValue<'db>(PhantomData<fn() -> &'db ()>);

#[salsa::tracked]
fn tracked_fn<'db>(_db: &'db dyn Db, _input: MyInput) -> NotSalsaValue<'db> {
    NotSalsaValue(PhantomData)
}

fn main() {}
