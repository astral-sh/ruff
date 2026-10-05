use salsa::Database as Db;

#[salsa::input]
struct MyInput {
    field: u32,
}

#[salsa::tracked(data = Data)]
fn tracked_fn_with_data(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[salsa::tracked(db = Db)]
fn tracked_fn_with_db(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[salsa::tracked(revisions = 12)]
fn tracked_fn_with_revisions(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[salsa::tracked(constructor = TrackedFn3)]
fn tracked_fn_with_constructor(db: &dyn Db, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[salsa::tracked]
fn tracked_fn_with_one_input(db: &dyn Db) -> u32 {}

#[salsa::tracked]
fn tracked_fn_with_receiver_not_applied_to_impl_block(&self, db: &dyn Db) -> u32 {}

#[salsa::tracked(specify)]
fn tracked_fn_with_too_many_arguments_for_specify(
    db: &dyn Db,
    input: MyInput,
    input: MyInput,
) -> u32 {
}

#[salsa::interned]
struct MyInterned<'db> {
    field: u32,
}

#[salsa::tracked]
fn tracked_fn_with_lt_param_and_elided_lt_on_db_arg1<'db>(
    db: &dyn Db,
    interned: MyInterned<'db>,
) -> u32 {
    interned.field(db) * 2
}

#[salsa::tracked]
fn tracked_fn_with_lt_param_and_elided_lt_on_db_arg2<'db_lifetime>(
    db: &dyn Db,
    interned: MyInterned<'db_lifetime>,
) -> u32 {
    interned.field(db) * 2
}

#[salsa::tracked]
fn tracked_fn_with_lt_param_and_elided_lt_on_input<'db>(
    db: &'db dyn Db,
    interned: MyInterned,
) -> u32 {
    interned.field(db) * 2
}

#[salsa::tracked]
fn tracked_fn_with_multiple_lts<'db1, 'db2>(db: &'db1 dyn Db, interned: MyInterned<'db2>) -> u32 {
    interned.field(db) * 2
}

fn main() {}
