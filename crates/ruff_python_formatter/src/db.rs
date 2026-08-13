use ruff_db::{Db as SourceDb, files::File};

use crate::PyFormatOptions;

#[salsa::db]
pub trait Db: SourceDb {
    /// Returns the formatting options
    fn format_options(&self, file: File) -> PyFormatOptions;
}
