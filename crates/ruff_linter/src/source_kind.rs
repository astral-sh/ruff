use std::fmt::Formatter;
use std::io;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use similar::{ChangeTag, TextDiff};
use thiserror::Error;

use ruff_diagnostics::SourceMap;
use ruff_notebook::{Cell, Notebook, NotebookError};
use ruff_python_ast::PySourceType;

use colored::Colorize;

use crate::fs;
use crate::text_helpers::ShowNonprinting;

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

    /// Returns a diff between the original and modified source code.
    ///
    /// Returns `None` if `self` and `other` are not of the same kind.
    pub fn diff<'a>(
        &'a self,
        other: &'a Self,
        path: Option<&'a Path>,
    ) -> Option<SourceKindDiff<'a>> {
        match (self, other) {
            (SourceKind::Python(src), SourceKind::Python(dst)) => Some(SourceKindDiff {
                kind: DiffKind::Python(src, dst),
                path,
            }),
            (SourceKind::IpyNotebook(src), SourceKind::IpyNotebook(dst)) => Some(SourceKindDiff {
                kind: DiffKind::IpyNotebook(src, dst),
                path,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceKindDiff<'a> {
    kind: DiffKind<'a>,
    path: Option<&'a Path>,
}

impl std::fmt::Display for SourceKindDiff<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            DiffKind::Python(original, modified) => {
                let mut diff = CodeDiff::new(original, modified);

                let relative_path = self.path.map(fs::relativize_path);

                if let Some(relative_path) = &relative_path {
                    diff.header(relative_path, relative_path);
                }

                writeln!(f, "{diff}")?;
            }
            DiffKind::IpyNotebook(original, modified) => {
                // Cell indices are 1-based.
                for ((idx, src_cell), dst_cell) in (1u32..)
                    .zip(original.cells().iter())
                    .zip(modified.cells().iter())
                {
                    let (Cell::Code(src_cell), Cell::Code(dst_cell)) = (src_cell, dst_cell) else {
                        continue;
                    };

                    let src_source_code = src_cell.source.to_string();
                    let dst_source_code = dst_cell.source.to_string();

                    let header = self.path.map_or_else(
                        || (format!("cell {idx}"), format!("cell {idx}")),
                        |path| {
                            (
                                format!("{}:cell {}", &fs::relativize_path(path), idx),
                                format!("{}:cell {}", &fs::relativize_path(path), idx),
                            )
                        },
                    );

                    let mut diff = CodeDiff::new(&src_source_code, &dst_source_code);
                    diff.header(&header.0, &header.1);

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
                    diff.missing_newline_hint(false);

                    write!(f, "{diff}")?;
                }

                writeln!(f)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum DiffKind<'a> {
    Python(&'a str, &'a str),
    IpyNotebook(&'a Notebook, &'a Notebook),
}

struct CodeDiff<'a> {
    diff: TextDiff<'a, 'a, 'a, str>,
    header: Option<(&'a str, &'a str)>,
    missing_newline_hint: bool,
}

impl<'a> CodeDiff<'a> {
    fn new(original: &'a str, modified: &'a str) -> Self {
        let diff = TextDiff::from_lines(original, modified);
        Self {
            diff,
            header: None,
            missing_newline_hint: true,
        }
    }

    fn header(&mut self, original: &'a str, modified: &'a str) {
        self.header = Some((original, modified));
    }

    fn missing_newline_hint(&mut self, missing_newline_hint: bool) {
        self.missing_newline_hint = missing_newline_hint;
    }
}

impl std::fmt::Display for CodeDiff<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some((original, modified)) = self.header {
            writeln!(f, "--- {}", original.show_nonprinting().red())?;
            writeln!(f, "+++ {}", modified.show_nonprinting().green())?;
        }

        let mut unified = self.diff.unified_diff();
        unified.missing_newline_hint(self.missing_newline_hint);

        // Individual hunks (section of changes)
        for hunk in unified.iter_hunks() {
            writeln!(f, "{}", hunk.header())?;

            // individual lines
            for change in hunk.iter_changes() {
                let value = change.value().show_nonprinting();
                match change.tag() {
                    ChangeTag::Equal => write!(f, " {value}")?,
                    ChangeTag::Delete => write!(f, "{}{}", "-".red(), value.red())?,
                    ChangeTag::Insert => write!(f, "{}{}", "+".green(), value.green())?,
                }

                if !self.diff.newline_terminated() {
                    writeln!(f)?;
                } else if change.missing_newline() {
                    if self.missing_newline_hint {
                        writeln!(f, "{}", "\n\\ No newline at end of file".red())?;
                    } else {
                        writeln!(f)?;
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum SourceError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notebook(#[from] NotebookError),
}
