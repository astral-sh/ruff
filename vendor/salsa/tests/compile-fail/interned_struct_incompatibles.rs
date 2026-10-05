#[salsa::interned(returns(ref))]
struct InternedWithRetRef {
    field: u32,
}

#[salsa::interned(specify)]
struct InternedWithSpecify {
    field: u32,
}

#[salsa::interned(no_eq)]
struct InternedWithNoEq {
    field: u32,
}

#[salsa::interned(db = Db)]
struct InternedWithDb {
    field: u32,
}

#[salsa::interned(recover_fn = recover)]
struct InternedWithRecover {
    field: u32,
}

#[salsa::interned(lru = 12)]
struct InternedWithLru {
    field: u32,
}

#[salsa::interned]
struct InternedWithTrackedField {
    #[tracked]
    field: u32,
}

#[salsa::interned(revisions = 0)]
struct InternedWithZeroRevisions {
    field: u32,
}

#[salsa::interned(no_lifetime, revisions = usize::MAX)]
struct InternedWithUnwrappedNoLifetime {
    field: u32,
}

#[salsa::interned(unsafe(no_lifetime))]
struct InternedWithReclaimableDefault {
    field: u32,
}

#[salsa::interned(unsafe(no_lifetime), revisions = 3)]
struct InternedWithReclaimableExplicit {
    field: u32,
}

fn main() {}
