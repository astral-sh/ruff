use ruff_db::{Db as SourceDb, Upcast, files::File};

use crate::PyFormatOptions;

#[salsa::db]
pub trait Db: SourceDb + Upcast<dyn SourceDb> {
    /// Returns the formatting options
    fn format_options(&self, file: File) -> PyFormatOptions;
}
