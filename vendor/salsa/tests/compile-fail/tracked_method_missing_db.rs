//! Regression test for issue #1058:
//! A `#[salsa::tracked]` method inside a `#[salsa::tracked]` impl that omits
//! the database parameter used to proc-macro panic with
//! "index out of bounds: the len is 0 but the index is 1".
//!
//! After the fix, the macro must surface a clear compile error instead of
//! panicking, telling the user they need to add the db parameter after self.
//!
//! The exact reproduction is taken from the bug report; the body is empty
//! because the macro never reaches type-checking the body — it bails out
//! during signature validation.

#[salsa::tracked]
struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked]
impl<'db> MyTracked<'db> {
    #[salsa::tracked]
    fn no_db(self) {}
}

fn main() {}
