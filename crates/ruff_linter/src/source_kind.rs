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

    /// Read the source kind from the given path.
    pub fn from_path(path: &Path, source_type: PySourceType) -> Result<Self, SourceReadError> {
        if source_type.is_ipynb() {
            let notebook = Notebook::from_path(path)?;
            Ok(Self::IpyNotebook(notebook))
        } else {
            let contents = std::fs::read_to_string(path)?;
            Ok(Self::Python(contents))
        }
    }

    /// Read the source kind from the given source code.
    pub fn from_source_code(
        source_code: String,
        source_type: PySourceType,
    ) -> Result<Self, SourceReadError> {
        if source_type.is_ipynb() {
            let notebook = Notebook::from_source_code(&source_code)?;
            Ok(Self::IpyNotebook(notebook))
        } else {
            Ok(Self::Python(source_code))
        }
    }

    /// Write the transformed source file to the given writer.
    ///
    /// For Jupyter notebooks, this will write out the notebook as JSON.
    pub fn write(&self, writer: &mut dyn Write) -> Result<(), SourceWriteError> {
        match self {
            SourceKind::Python(source) => writer.write_all(source.as_bytes()).map_err(Into::into),
            SourceKind::IpyNotebook(notebook) => notebook.write(writer).map_err(Into::into),
        }
    }
}

#[derive(Error, Debug)]
pub enum SourceReadError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notebook(#[from] NotebookError),
}

#[derive(Error, Debug)]
pub enum SourceWriteError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Notebook(#[from] NotebookError),
}
