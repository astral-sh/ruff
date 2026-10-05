//! Regression test for the follow-up to issue #1058:
//! A `#[salsa::tracked]` method inside a `#[salsa::tracked]`
//! impl with **zero** arguments (no `self`, no database) used
//! to proc-macro panic at the `inputs[0]` indexing site in
//! `validity_check`, before reaching the `.get(db_input_index)`
//! guard added by the original fix.
//!
//! After this fix, the macro must surface a clear compile error
//! instead of panicking.

#[salsa::tracked]
struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked]
impl<'db> MyTracked<'db> {
    #[salsa::tracked]
    fn no_args() {}
}

fn main() {}