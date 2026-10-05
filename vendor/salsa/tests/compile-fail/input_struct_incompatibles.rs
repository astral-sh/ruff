#[salsa::input(returns(ref))]
struct InputWithRetRef(u32);

#[salsa::input(specify)]
struct InputWithSpecify(u32);

#[salsa::input(no_eq)]
struct InputNoWithEq(u32);

#[salsa::input(db = Db)]
struct InputWithDb(u32);

#[salsa::input(recover_fn = recover)]
struct InputWithRecover(u32);

#[salsa::input(lru =12)]
struct InputWithLru(u32);

#[salsa::input(revisions = 12)]
struct InputWithRevisions(u32);

#[salsa::input]
struct InputWithTrackedField {
    #[tracked]
    field: u32,
}

fn main() {}
