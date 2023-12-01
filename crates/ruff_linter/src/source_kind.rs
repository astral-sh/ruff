use std::io;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use similar::TextDiff;
use thiserror::Error;

use ruff_diagnostics::SourceMap;
use ruff_notebook::{Cell, Notebook, NotebookError};
use ruff_python_ast::PySourceType;

use crate::fs;

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum SourceKind {
    /// The source contains Python source code.
    Python(String),
    /// The source contains a Jupyter notebook.
    IpyNotebook(Notebook),
}

impl SourceKind {
    #[must_use]
    pub(crate) fn updated(&self, new_source: String, source_map: &SourceMap) -> Self {
        match self {
            SourceKind::IpyNotebook(notebook) => {
                let mut cloned = notebook.clone();
                cloned.update(source_map, new_source);
                SourceKind::IpyNotebook(cloned)
            }
            SourceKind::Python(_) => SourceKind::Python(new_source),
        }
    }

    /// Returns the Python source code for this source kind.
    pub fn source_code(&self) -> &str {
        match self {
            SourceKind::Python(source) => source,
            SourceKind::IpyNotebook(notebook) => notebook.source_code(),
        }
    }

    /// Read the [`SourceKind`] from the given path. Returns `None` if the source is not a Python
    /// source file.
    pub fn from_path(path: &Path, source_type: PySourceType) -> Result<Option<Self>, SourceError> {
        if source_type.is_ipynb() {
            let notebook = Notebook::from_path(path)?;
            Ok(notebook
                .is_python_notebook()
                .then_some(Self::IpyNotebook(notebook)))
        } else {
            let contents = std::fs::read_to_string(path)?;
            Ok(Some(Self::Python(contents)))
        }
    }

    /// Read the [`SourceKind`] from the given source code. Returns `None` if the source is not
    /// Python source code.
    pub fn from_source_code(
        source_code: String,
        source_type: PySourceType,
    ) -> Result<Option<Self>, SourceError> {
        if source_type.is_ipynb() {
            let notebook = Notebook::from_source_code(&source_code)?;
            Ok(notebook
                .is_python_notebook()
                .then_some(Self::IpyNotebook(notebook)))
        } else {
            Ok(Some(Self::Python(source_code)))
        }
    }

    /// Write the transformed source file to the given writer.
    ///
    /// For Jupyter notebooks, this will write out the notebook as JSON.
    pub fn write(&self, writer: &mut dyn Write) -> Result<(), SourceError> {
        match self {
            SourceKind::Python(source) => {
                writer.write_all(source.as_bytes())?;
                Ok(())
            }
            SourceKind::IpyNotebook(notebook) => {
                notebook.write(writer)?;
                Ok(())
            }
        }
    }

    /// Write a diff of the transformed source file to `stdout`.
    pub fn diff(
        &self,
        other: &Self,
        path: Option<&Path>,
        writer: &mut dyn Write,
    ) -> io::Result<()> {
        match (self, other) {
            (SourceKind::Python(src), SourceKind::Python(dst)) => {
                let text_diff = TextDiff::from_lines(src, dst);
                let mut unified_diff = text_diff.unified_diff();

                if let Some(path) = path {
                    unified_diff.header(&fs::relativize_path(path), &fs::relativize_path(path));
                }

                unified_diff.to_writer(&mut *writer)?;

                writer.write_all(b"\n")?;
                writer.flush()?;

                Ok(())
            }
            (SourceKind::IpyNotebook(src), SourceKind::IpyNotebook(dst)) => {
                // Cell indices are 1-based.
                for ((idx, src_cell), dst_cell) in
                    (1u32..).zip(src.cells().iter()).zip(dst.cells().iter())
                {
                    let (Cell::Code(src_cell), Cell::Code(dst_cell)) = (src_cell, dst_cell) else {
                        continue;
                    };

                    let src_source_code = src_cell.source.to_string();
                    let dst_source_code = dst_cell.source.to_string();

                    let text_diff = TextDiff::from_lines(&src_source_code, &dst_source_code);
                    let mut unified_diff = text_diff.unified_diff();

                    // Jupyter notebook cells don't necessarily have a newline
                    // at the end. For example,
                    //
                    // ```python
                    // print("hello")
                    // ```
                    //
                    // For a cell containing the above code, there'll only be one line,
                    // and it won't have a newline at the end. If it did, there'd be
                    // two lines, and the second line would be empty:
                    //
                    // ```python
                    // print("hello")
                    //
                    // ```
                    unified_diff.missing_newline_hint(false);

                    if let Some(path) = path {
                        unified_diff.header(
                            &format!("{}:cell {}", &fs::relativize_path(path), idx),
                            &format!("{}:cell {}", &fs::relativize_path(path), idx),
                        );
                    } else {
                        unified_diff.header(&format!("cell {idx}"), &format!("cell {idx}"));
                    };

                    unified_diff.to_writer(&mut *writer)?;
                }

                writer.write_all(b"\n")?;
                writer.flush()?;

                Ok(())
            }
            _ => panic!("cannot diff Python source code with Jupyter notebook source code"),
        }
    }
}

#[derive(Error, Debug)]
pub enum SourceError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notebook(#[from] NotebookError),
}
