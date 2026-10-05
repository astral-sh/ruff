#![cfg(feature = "inventory")]

//! Test that debug and non-debug structs compile correctly

#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Hash, salsa::SalsaValue)]
struct NotDebug;
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Hash, Debug, salsa::SalsaValue)]
struct Debug;

#[salsa::input(debug)]
struct DebugInput {
    #[returns(copy)]
    field: Debug,
}

#[salsa::input]
struct NotDebugInput {
    #[returns(copy)]
    field: NotDebug,
}

#[salsa::interned(debug)]
struct DebugInterned {
    #[returns(copy)]
    field: Debug,
}

#[salsa::interned]
struct NotDebugInterned {
    #[returns(copy)]
    field: NotDebug,
}

#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
struct DebugInternedNoLifetime {
    #[returns(copy)]
    field: Debug,
}

#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX)]
struct NotDebugInternedNoLifetime {
    #[returns(copy)]
    field: NotDebug,
}

#[salsa::tracked(debug)]
struct DebugTracked<'db> {
    #[returns(copy)]
    field: Debug,
}

#[salsa::tracked]
struct NotDebugTracked<'db> {
    #[returns(copy)]
    field: NotDebug,
}

#[test]
fn ok() {}
