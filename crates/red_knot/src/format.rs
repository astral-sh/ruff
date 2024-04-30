use std::ops::{Deref, DerefMut};

use ruff_formatter::PrintedRange;
use ruff_python_formatter::{FormatModuleError, PyFormatOptions};
use ruff_text_size::TextRange;

use crate::cache::KeyValueCache;
use crate::db::{HasJar, QueryError, SourceDb};
use crate::files::FileId;
use crate::FxDashSet;

pub(crate) trait FormatDb: SourceDb {
    // TODO we may want to change the return type to something that avoids allocating a string if the code is already formatted.
    fn format_file(&self, file_id: FileId) -> Result<String, FormatError>;

    fn format_file_range(
        &self,
        file_id: FileId,
        range: TextRange,
    ) -> Result<PrintedRange, FormatError>;

    fn check_file_formatted(&self, file_id: FileId) -> Result<bool, FormatError>;
}

pub(crate) fn format_file<Db>(db: &Db, file_id: FileId) -> Result<String, FormatError>
where
    Db: FormatDb + HasJar<FormatJar>,
{
    let formatted = &db.jar()?.formatted;
    let source = db.source(file_id)?;

    if formatted.contains(&file_id) {
        return Ok(String::from(source.text()));
    }

    // TODO use the `format_module` method here to re-use the AST.
    let printed =
        ruff_python_formatter::format_module_source(source.text(), PyFormatOptions::default())?;

    // Formatting is fast and unlikely to run in parallel. Let threads race the formatting.
    formatted.insert(file_id);

    Ok(printed.into_code())
}

pub(crate) fn check_formatted<Db>(db: &Db, file_id: FileId) -> Result<bool, FormatError>
where
    Db: FormatDb + HasJar<FormatJar>,
{
    let formatted = &db.jar()?.formatted;

    if formatted.contains(&file_id) {
        return Ok(true);
    }

    let formatted_code = format_file(db, file_id)?;

    Ok(formatted_code == db.source(file_id)?.text())
}

pub(crate) fn format_file_range<Db: FormatDb + HasJar<FormatJar>>(
    db: &Db,
    file_id: FileId,
    range: TextRange,
) -> Result<PrintedRange, FormatError> {
    let formatted = &db.jar()?.formatted;
    let source = db.source(file_id)?;

    if formatted.contains(&file_id) {
        return Ok(PrintedRange::new(source.text()[range].into(), range));
    }

    // TODO use the `format_module` method here to re-use the AST.

    let result =
        ruff_python_formatter::format_range(source.text(), range, PyFormatOptions::default())?;
    Ok(result)
}

#[derive(Debug)]
pub(crate) enum FormatError {
    Format(FormatModuleError),
    Query(QueryError),
}

impl From<FormatModuleError> for FormatError {
    fn from(value: FormatModuleError) -> Self {
        Self::Format(value.into())
    }
}

impl From<QueryError> for FormatError {
    fn from(value: QueryError) -> Self {
        Self::Query(value)
    }
}

#[derive(Debug, Default)]
pub struct FormatJar {
    pub formatted: FxDashSet<FileId>,
}

#[derive(Default, Debug)]
pub(crate) struct FormattedStorage(KeyValueCache<FileId, ()>);

impl Deref for FormattedStorage {
    type Target = KeyValueCache<FileId, ()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FormattedStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
