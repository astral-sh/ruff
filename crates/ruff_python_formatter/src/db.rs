use ruff_db::{files::File, Db as SourceDb, Upcast};

use crate::PyFormatOptions;

#[salsa::db]
pub trait Db: SourceDb + Upcast<dyn SourceDb> {
    /// Returns the formatting options
    fn format_options(&self, file: File) -> PyFormatOptions;
}
