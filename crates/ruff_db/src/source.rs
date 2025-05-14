use std::ops::Deref;
use std::sync::Arc;

use countme::Count;

use ruff_notebook::Notebook;
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;

use crate::files::{File, FilePath};
use crate::Db;

/// Reads the source text of a python text file (must be valid UTF8) or notebook.
#[salsa::tracked]
pub fn source_text(db: &dyn Db, file: File) -> SourceText {
    let path = file.path(db);
    let _span = tracing::trace_span!("source_text", file = %path).entered();
    let mut read_error = None;

    let kind = if is_notebook(file.path(db)) {
        file.read_to_notebook(db)
            .unwrap_or_else(|error| {
                tracing::debug!("Failed to read notebook '{path}': {error}");

                read_error = Some(SourceTextError::FailedToReadNotebook(error.to_string()));
                Notebook::empty()
            })
            .into()
    } else {
        file.read_to_string(db)
            .unwrap_or_else(|error| {
                tracing::debug!("Failed to read file '{path}': {error}");

                read_error = Some(SourceTextError::FailedToReadFile(error.to_string()));
                String::new()
            })
            .into()
    };

    SourceText {
        inner: Arc::new(SourceTextInner {
            kind,
            read_error,
            count: Count::new(),
        }),
    }
}

fn is_notebook(path: &FilePath) -> bool {
    match path {
        FilePath::System(system) => system.extension().is_some_and(|extension| {
            PySourceType::try_from_extension(extension) == Some(PySourceType::Ipynb)
        }),
        FilePath::SystemVirtual(system_virtual) => {
            system_virtual.extension().is_some_and(|extension| {
                PySourceType::try_from_extension(extension) == Some(PySourceType::Ipynb)
            })
        }
        FilePath::Vendored(_) => false,
    }
}

/// The source text of a file containing python code.
///
/// The file containing the source text can either be a text file or a notebook.
///
/// Cheap cloneable in `O(1)`.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceText {
    inner: Arc<SourceTextInner>,
}

impl SourceText {
    /// Returns the python code as a `str`.
    pub fn as_str(&self) -> &str {
        match &self.inner.kind {
            SourceTextKind::Text(source) => source,
            SourceTextKind::Notebook(notebook) => notebook.source_code(),
        }
    }

    /// Returns the underlying notebook if this is a notebook file.
    pub fn as_notebook(&self) -> Option<&Notebook> {
        match &self.inner.kind {
            SourceTextKind::Notebook(notebook) => Some(notebook),
            SourceTextKind::Text(_) => None,
        }
    }

    /// Returns `true` if this is a notebook source file.
    pub fn is_notebook(&self) -> bool {
        matches!(&self.inner.kind, SourceTextKind::Notebook(_))
    }

    /// Returns `true` if there was an error when reading the content of the file.
    pub fn read_error(&self) -> Option<&SourceTextError> {
        self.inner.read_error.as_ref()
    }
}

impl Deref for SourceText {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for SourceText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_tuple("SourceText");

        match &self.inner.kind {
            SourceTextKind::Text(text) => {
                dbg.field(text);
            }
            SourceTextKind::Notebook(notebook) => {
                dbg.field(notebook);
            }
        }

        dbg.finish()
    }
}

#[derive(Eq, PartialEq)]
struct SourceTextInner {
    count: Count<SourceText>,
    kind: SourceTextKind,
    read_error: Option<SourceTextError>,
}

#[derive(Eq, PartialEq)]
enum SourceTextKind {
    Text(String),
    Notebook(Notebook),
}

impl From<String> for SourceTextKind {
    fn from(value: String) -> Self {
        SourceTextKind::Text(value)
    }
}

impl From<Notebook> for SourceTextKind {
    fn from(notebook: Notebook) -> Self {
        SourceTextKind::Notebook(notebook)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum SourceTextError {
    #[error("Failed to read notebook: {0}`")]
    FailedToReadNotebook(String),
    #[error("Failed to read file: {0}")]
    FailedToReadFile(String),
}

/// Computes the [`LineIndex`] for `file`.
#[salsa::tracked]
pub fn line_index(db: &dyn Db, file: File) -> LineIndex {
    let _span = tracing::trace_span!("line_index", ?file).entered();

    let source = source_text(db, file);

    LineIndex::from_source_text(&source)
}

#[cfg(test)]
mod tests {
    use salsa::EventKind;
    use salsa::Setter as _;

    use ruff_source_file::OneIndexed;
    use ruff_text_size::TextSize;

    use crate::files::system_path_to_file;
    use crate::source::{line_index, source_text};
    use crate::system::{DbWithWritableSystem as _, SystemPath};
    use crate::tests::TestDb;

    #[test]
    fn re_runs_query_when_file_revision_changes() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(source_text(&db, file).as_str(), "x = 10");

        db.write_file(path, "x = 20").unwrap();

        assert_eq!(source_text(&db, file).as_str(), "x = 20");

        Ok(())
    }

    #[test]
    fn text_is_cached_if_revision_is_unchanged() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(source_text(&db, file).as_str(), "x = 10");

        // Change the file permission only
        file.set_permissions(&mut db).to(Some(0o777));

        db.clear_salsa_events();
        assert_eq!(source_text(&db, file).as_str(), "x = 10");

        let events = db.take_salsa_events();

        assert!(!events
            .iter()
            .any(|event| matches!(event.kind, EventKind::WillExecute { .. })));

        Ok(())
    }

    #[test]
    fn line_index_for_source() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10\ny = 20")?;

        let file = system_path_to_file(&db, path).unwrap();
        let index = line_index(&db, file);
        let source = source_text(&db, file);

        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index.line_start(OneIndexed::from_zero_indexed(0), source.as_str()),
            TextSize::new(0)
        );

        Ok(())
    }

    #[test]
    fn notebook() -> crate::system::Result<()> {
        let mut db = TestDb::new();

        let path = SystemPath::new("test.ipynb");
        db.write_file(
            path,
            r#"
{
    "cells": [{"cell_type": "code", "source": ["x = 10"], "metadata": {}, "outputs": []}],
    "metadata": {
        "kernelspec": {
            "display_name": "Python (ruff)",
            "language": "python",
            "name": "ruff"
        },
        "language_info": {
            "file_extension": ".py",
            "mimetype": "text/x-python",
            "name": "python",
            "nbconvert_exporter": "python",
            "pygments_lexer": "ipython3",
            "version": "3.11.3"
        }
     },
     "nbformat": 4,
     "nbformat_minor": 4
}"#,
        )?;

        let file = system_path_to_file(&db, path).unwrap();
        let source = source_text(&db, file);

        assert!(source.is_notebook());
        assert_eq!(source.as_str(), "x = 10\n");
        assert!(source.as_notebook().is_some());

        Ok(())
    }
}
