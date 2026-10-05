#[salsa::tracked(returns(ref))]
struct TrackedWithRetRef {
    field: u32,
}

#[salsa::tracked(specify)]
struct TrackedSructWithSpecify {
    field: u32,
}

#[salsa::tracked(no_eq)]
struct TrackedStructWithNoEq {
    field: u32,
}

#[salsa::tracked(db = Db)]
struct TrackedStructWithDb {
    field: u32,
}

#[salsa::tracked(recover_fn = recover)]
struct TrackedStructWithRecover {
    field: u32,
}

#[salsa::tracked(lru = 12)]
struct TrackedStructWithLru {
    field: u32,
}

#[salsa::tracked(revisions = 12)]
struct TrackedStructWithRevisions {
    field: u32,
}

fn main() {}
