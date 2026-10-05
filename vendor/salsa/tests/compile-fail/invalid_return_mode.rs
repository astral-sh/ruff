use salsa::Database as Db;

#[salsa::input]
struct MyInput {
    #[returns(clone)]
    text: String,
}

#[salsa::tracked(returns(not_a_return_mode))]
fn tracked_fn_invalid_return_mode(db: &dyn Db, input: MyInput) -> String {
    input.text(db)
}

#[salsa::input]
struct MyInvalidInput {
    #[returns(not_a_return_mode)]
    text: String,
}

fn main() { }