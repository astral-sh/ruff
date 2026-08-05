use std::borrow::Cow;
use std::ops::Deref;
use std::sync::{Arc, Weak};

use arc_swap::ArcSwapWeak;
use ruff_diagnostics::SourceMap;
use ruff_notebook::Notebook;
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;

use crate::Db;
use crate::files::{File, FilePath};
use crate::system::System;

/// Reads the source text of a python text file (must be valid UTF8) or notebook.
#[salsa::tracked(returns(clone), heap_size=ruff_memory_usage::heap_size)]
pub fn source_text(db: &dyn Db, file: File) -> SourceText {
    let path = file.path(db);
    let _span = tracing::trace_span!("source_text", file = %path).entered();
    let mut read_error = None;

    if let Some(source) = file.source_text_override(db) {
        return source.clone();
    }

    let kind = if is_notebook(db.system(), path) {
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
        inner: Arc::new(SourceTextInner { kind, read_error }),
    }
}

fn is_notebook(system: &dyn System, path: &FilePath) -> bool {
    let source_type = match path {
        FilePath::System(path) => system.source_type(path),
        FilePath::SystemVirtual(system_virtual) => system.virtual_path_source_type(system_virtual),
        FilePath::Vendored(_) => return false,
    };

    let with_extension_fallback =
        source_type.or_else(|| PySourceType::try_from_extension(path.extension()?));

    with_extension_fallback == Some(PySourceType::Ipynb)
}

/// The source text of a file containing python code.
///
/// The file containing the source text can either be a text file or a notebook.
///
/// Cheap cloneable in `O(1)`.
#[derive(Clone, get_size2::GetSize)]
pub struct SourceText {
    inner: Arc<SourceTextInner>,
}

impl SourceText {
    /// Loads the source text, keeping it decompressed until the returned handle is dropped.
    ///
    /// Concurrent handles share the same decompressed text. The cache only retains a weak
    /// reference, so its backing allocation is released with the final handle.
    pub fn load(&self) -> SourceTextRef {
        let kind = match &self.inner.kind {
            SourceTextKind::Text(source) => LoadedSourceTextKind::Text(source.load()),
            SourceTextKind::Notebook { notebook } => {
                LoadedSourceTextKind::Notebook(Arc::clone(notebook))
            }
        };

        SourceTextRef {
            source: self.clone(),
            kind,
        }
    }

    /// Returns the underlying notebook if this is a notebook file.
    pub fn as_notebook(&self) -> Option<&Notebook> {
        match &self.inner.kind {
            SourceTextKind::Notebook { notebook } => Some(notebook),
            SourceTextKind::Text(_) => None,
        }
    }

    /// Returns `true` if this is a notebook source file.
    pub fn is_notebook(&self) -> bool {
        matches!(&self.inner.kind, SourceTextKind::Notebook { .. })
    }

    /// Returns `true` if there was an error when reading the content of the file.
    pub fn read_error(&self) -> Option<&SourceTextError> {
        self.inner.read_error.as_ref()
    }

    /// Returns a new instance for this file with the updated source text (Python code).
    ///
    /// Uses the `source_map` to preserve the cell-boundaries.
    #[must_use]
    pub fn with_text(&self, new_text: String, source_map: &SourceMap) -> Self {
        let new_kind = match &self.inner.kind {
            SourceTextKind::Text(_) => new_text.into(),

            SourceTextKind::Notebook { notebook } => {
                let mut new_notebook = notebook.as_ref().clone();
                new_notebook.update(source_map, new_text);
                SourceTextKind::Notebook {
                    notebook: new_notebook.into(),
                }
            }
        };

        Self {
            inner: Arc::new(SourceTextInner {
                kind: new_kind,
                read_error: self.inner.read_error.clone(),
            }),
        }
    }

    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        match &self.inner.kind {
            SourceTextKind::Text(_) => Cow::Owned(self.load().as_str().as_bytes().to_vec()),
            SourceTextKind::Notebook { notebook } => {
                let mut output: Vec<u8> = Vec::new();
                notebook
                    .write(&mut output)
                    .expect("writing to a Vec should never fail");

                Cow::Owned(output)
            }
        }
    }
}

impl PartialEq for SourceText {
    fn eq(&self, other: &Self) -> bool {
        self.inner.kind == other.inner.kind && self.inner.read_error == other.inner.read_error
    }
}

impl Eq for SourceText {}

/// A loaded source-text handle that keeps its decompressed contents alive.
#[derive(Clone)]
pub struct SourceTextRef {
    source: SourceText,
    kind: LoadedSourceTextKind,
}

#[derive(Clone)]
enum LoadedSourceTextKind {
    Text(Arc<String>),
    Notebook(Arc<Notebook>),
}

impl SourceTextRef {
    /// Returns the Python source code.
    pub fn as_str(&self) -> &str {
        match &self.kind {
            LoadedSourceTextKind::Text(text) => text,
            LoadedSourceTextKind::Notebook(notebook) => notebook.source_code(),
        }
    }

    /// Returns the notebook when this handle represents a notebook file.
    pub fn as_notebook(&self) -> Option<&Notebook> {
        self.source.as_notebook()
    }

    /// Returns `true` when this handle represents a notebook file.
    pub fn is_notebook(&self) -> bool {
        self.source.is_notebook()
    }

    /// Returns the error encountered while reading this source file, if any.
    pub fn read_error(&self) -> Option<&SourceTextError> {
        self.source.read_error()
    }
}

impl Deref for SourceTextRef {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for SourceTextRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::fmt::Debug for SourceText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_tuple("SourceText");

        match &self.inner.kind {
            SourceTextKind::Text(_) => {
                dbg.field(&self.load().as_str());
            }
            SourceTextKind::Notebook { notebook } => {
                dbg.field(notebook);
            }
        }

        dbg.finish()
    }
}

#[derive(get_size2::GetSize)]
struct SourceTextInner {
    kind: SourceTextKind,
    read_error: Option<SourceTextError>,
}

#[derive(PartialEq, get_size2::GetSize)]
enum SourceTextKind {
    Text(CompressedSourceText),
    Notebook {
        // Jupyter notebooks are not very relevant for memory profiling, and contain
        // arbitrary JSON values that do not implement the `GetSize` trait.
        #[get_size(ignore)]
        notebook: Arc<Notebook>,
    },
}

#[derive(get_size2::GetSize)]
struct CompressedSourceText {
    compressed: Box<[u8]>,
    uncompressed_len: usize,
    #[get_size(ignore)]
    decoded: ArcSwapWeak<String>,
}

impl CompressedSourceText {
    fn new(source: String) -> Self {
        Self {
            compressed: lz4_flex::block::compress(source.as_bytes()).into_boxed_slice(),
            uncompressed_len: source.len(),
            decoded: ArcSwapWeak::new(Weak::new()),
        }
    }

    fn load(&self) -> Arc<String> {
        if let Some(decoded) = self.decoded.load().upgrade() {
            return decoded;
        }

        let bytes = lz4_flex::block::decompress(&self.compressed, self.uncompressed_len)
            .expect("source text was compressed by the same LZ4 implementation");
        let text = String::from_utf8(bytes)
            .expect("compressed source text was constructed from a valid UTF-8 string");
        let decoded = Arc::new(text);

        loop {
            let cached = self.decoded.load();

            if let Some(existing) = cached.upgrade() {
                return existing;
            }

            // Concurrent cache misses may decompress the same source, but publishing only one
            // allocation ensures all overlapping handles share the same decoded text.
            let previous = self
                .decoded
                .compare_and_swap(&cached, Arc::downgrade(&decoded));

            if Weak::ptr_eq(&previous, &cached) {
                return decoded;
            }

            if let Some(existing) = previous.upgrade() {
                return existing;
            }
        }
    }
}

impl PartialEq for CompressedSourceText {
    fn eq(&self, other: &Self) -> bool {
        self.uncompressed_len == other.uncompressed_len && self.compressed == other.compressed
    }
}

impl From<String> for SourceTextKind {
    fn from(value: String) -> Self {
        SourceTextKind::Text(CompressedSourceText::new(value))
    }
}

impl From<Notebook> for SourceTextKind {
    fn from(notebook: Notebook) -> Self {
        SourceTextKind::Notebook {
            notebook: Arc::new(notebook),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, get_size2::GetSize)]
pub enum SourceTextError {
    #[error("Failed to read notebook: {0}`")]
    FailedToReadNotebook(String),
    #[error("Failed to read file: {0}")]
    FailedToReadFile(String),
}

/// Computes the [`LineIndex`] for `file`.
#[salsa::tracked(returns(clone), heap_size=ruff_memory_usage::heap_size)]
pub fn line_index(db: &dyn Db, file: File) -> LineIndex {
    let _span = tracing::trace_span!("line_index", ?file).entered();

    let source = source_text(db, file).load();

    LineIndex::from_source_text(&source)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use salsa::EventKind;
    use salsa::Setter as _;

    use ruff_source_file::OneIndexed;
    use ruff_text_size::TextSize;

    use crate::files::system_path_to_file;
    use crate::source::{CompressedSourceText, SourceTextKind, line_index, source_text};
    use crate::system::{DbWithWritableSystem as _, SystemPath};
    use crate::tests::TestDb;

    #[test]
    fn re_runs_query_when_file_revision_changes() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(source_text(&db, file).load().as_str(), "x = 10");

        db.write_file(path, "x = 20").unwrap();

        assert_eq!(source_text(&db, file).load().as_str(), "x = 20");

        Ok(())
    }

    #[test]
    fn text_is_cached_if_revision_is_unchanged() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(source_text(&db, file).load().as_str(), "x = 10");

        // Change the file permission only
        file.set_permissions(&mut db).to(Some(0o777));

        db.clear_salsa_events();
        assert_eq!(source_text(&db, file).load().as_str(), "x = 10");

        let events = db.take_salsa_events();

        assert!(
            !events
                .iter()
                .any(|event| matches!(event.kind, EventKind::WillExecute { .. }))
        );

        Ok(())
    }

    #[test]
    fn loaded_source_is_shared_and_automatically_evicted() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");
        db.write_file(path, "x = 10\n")?;

        let file = system_path_to_file(&db, path).unwrap();
        let source = source_text(&db, file);
        let SourceTextKind::Text(compressed) = &source.inner.kind else {
            panic!("a Python source file should not be loaded as a notebook");
        };

        assert!(compressed.decoded.load().upgrade().is_none());

        let first = source.load();
        let second = source.load();
        assert_eq!(first.as_str(), "x = 10\n");
        assert!(compressed.decoded.load().upgrade().is_some());

        drop(first);
        assert!(compressed.decoded.load().upgrade().is_some());

        drop(second);
        assert!(compressed.decoded.load().upgrade().is_none());

        let reloaded = source.load();
        assert_eq!(reloaded.as_str(), "x = 10\n");

        Ok(())
    }

    #[test]
    fn concurrent_handles_share_decompressed_source() {
        let source = Arc::new(CompressedSourceText::new("x = 10\n".repeat(256)));
        let barrier = Arc::new(Barrier::new(8));

        let loaded = std::thread::scope(|scope| {
            let mut threads = Vec::new();

            for _ in 0..8 {
                let source = Arc::clone(&source);
                let barrier = Arc::clone(&barrier);

                threads.push(scope.spawn(move || {
                    barrier.wait();
                    source.load()
                }));
            }

            threads
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect::<Vec<_>>()
        });

        assert!(
            loaded
                .windows(2)
                .all(|pair| Arc::ptr_eq(&pair[0], &pair[1]))
        );

        drop(loaded);
        assert!(source.decoded.load().upgrade().is_none());
    }

    #[test]
    fn line_index_for_source() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10\ny = 20")?;

        let file = system_path_to_file(&db, path).unwrap();
        let index = line_index(&db, file);
        let source = source_text(&db, file).load();

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
        let source = source_text(&db, file).load();

        assert!(source.is_notebook());
        assert_eq!(source.as_str(), "x = 10\n");
        assert!(source.as_notebook().is_some());

        Ok(())
    }
}
