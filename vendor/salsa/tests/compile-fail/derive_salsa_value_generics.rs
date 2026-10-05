use std::marker::PhantomData;

#[derive(salsa::SalsaValue)]
struct Generic<T>(T);

#[derive(salsa::SalsaValue)]
struct ConstGeneric<const N: usize>([u8; N]);

struct NotSalsaValue<'db>(PhantomData<&'db ()>);

fn assert_salsa_value<T: salsa::SalsaValue>() {}

fn rejects_reference<'db>(_: &'db ()) {
    assert_salsa_value::<Generic<NotSalsaValue<'db>>>();
}

fn main() {
    assert_salsa_value::<Generic<String>>();
    assert_salsa_value::<ConstGeneric<4>>();
}
