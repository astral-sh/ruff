#[salsa::input]
struct MyInput {
    field: u32,
}

impl MyInput {
    #[salsa::tracked]
    fn tracked_method_on_untracked_impl(self, db: &dyn Db) -> u32 {
        input.field(db)
    }
}

fn main() {}
