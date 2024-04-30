use std::ops::{Deref, DerefMut};

use ruff_formatter::PrintedRange;
use ruff_python_formatter::{FormatModuleError, PyFormatOptions};
use ruff_text_size::TextRange;

use crate::cache::KeyValueCache;
use crate::db::{HasJar, QueryError, SourceDb};
use crate::files::FileId;
use crate::lint::Diagnostics;
use crate::FxDashSet;

pub(crate) trait FormatDb: SourceDb {
    /// Formats a file and returns its formatted content or an indicator that it is unchanged.
    fn format_file(&self, file_id: FileId) -> Result<FormattedFile, FormatError>;

    /// Formats a range in a file.
    fn format_file_range(
        &self,
        file_id: FileId,
        range: TextRange,
    ) -> Result<PrintedRange, FormatError>;

    fn check_file_formatted(&self, file_id: FileId) -> Result<Diagnostics, FormatError>;
}

#[tracing::instrument(level = "trace", skip(db))]
pub(crate) fn format_file<Db>(db: &Db, file_id: FileId) -> Result<FormattedFile, FormatError>
where
    Db: FormatDb + HasJar<FormatJar>,
{
    let formatted = &db.jar()?.formatted;

    if formatted.contains(&file_id) {
        return Ok(FormattedFile::Unchanged);
    }

    let source = db.source(file_id)?;

    // TODO use the `format_module` method here to re-use the AST.
    let printed =
        ruff_python_formatter::format_module_source(source.text(), PyFormatOptions::default())?;

    Ok(if printed.as_code() == source.text() {
        formatted.insert(file_id);
        FormattedFile::Unchanged
    } else {
        FormattedFile::Formatted(printed.into_code())
    })
}

#[tracing::instrument(level = "trace", skip(db))]
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

/// Checks if the file is correctly formatted. It creates a diagnostic for formatting issues.
#[tracing::instrument(level = "trace", skip(db))]
pub(crate) fn check_formatted<Db>(db: &Db, file_id: FileId) -> Result<Diagnostics, FormatError>
where
    Db: FormatDb + HasJar<FormatJar>,
{
    Ok(if db.format_file(file_id)?.is_unchanged() {
        Diagnostics::Empty
    } else {
        Diagnostics::from(vec!["File is not formatted".to_string()])
    })
}

#[derive(Debug)]
pub(crate) enum FormatError {
    Format(FormatModuleError),
    Query(QueryError),
}

impl From<FormatModuleError> for FormatError {
    fn from(value: FormatModuleError) -> Self {
        Self::Format(value)
    }
}

impl From<QueryError> for FormatError {
    fn from(value: QueryError) -> Self {
        Self::Query(value)
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum FormattedFile {
    Formatted(String),
    Unchanged,
}

impl FormattedFile {
    pub(crate) const fn is_unchanged(&self) -> bool {
        matches!(self, FormattedFile::Unchanged)
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
