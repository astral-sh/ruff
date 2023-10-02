use std::io;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use thiserror::Error;

use ruff_diagnostics::SourceMap;
use ruff_notebook::{Notebook, NotebookError};
use ruff_python_ast::PySourceType;

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
}

#[derive(Error, Debug)]
pub enum SourceError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notebook(#[from] NotebookError),
}
