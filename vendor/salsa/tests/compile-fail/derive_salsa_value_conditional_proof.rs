use std::marker::PhantomData;

struct Foreign<T>(T);

#[derive(salsa::SalsaValue)]
struct ConditionallySafe<T> {
    #[salsa_value(unsafe(prove(T: salsa::SalsaValue)))]
    value: Foreign<T>,
}

#[derive(salsa::SalsaValue)]
struct ConditionallyStatic<T> {
    #[salsa_value(unsafe(prove(T: 'static)))]
    value: Foreign<T>,
}

struct NotSalsaValue<'db>(PhantomData<&'db ()>);

fn assert_salsa_value<T: salsa::SalsaValue>() {}

fn rejects_unsatisfied_salsa_value_proof<'db>() {
    assert_salsa_value::<ConditionallySafe<NotSalsaValue<'db>>>();
}

fn rejects_unsatisfied_static_proof<'db>() {
    assert_salsa_value::<ConditionallyStatic<&'db ()>>();
}

fn main() {}
