use std::path::Path;

use ruff_python_parser::Mode;

use crate::jupyter::Notebook;

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum SourceKind {
    Python(String),
    Jupyter(Notebook),
}

impl SourceKind {
    /// Return the source content.
    pub fn content(&self) -> &str {
        match self {
            SourceKind::Python(content) => content,
            SourceKind::Jupyter(notebook) => notebook.content(),
        }
    }

    /// Return the [`Notebook`] if the source kind is [`SourceKind::Jupyter`].
    pub fn notebook(&self) -> Option<&Notebook> {
        if let Self::Jupyter(notebook) = self {
            Some(notebook)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PySourceType {
    #[default]
    Python,
    Stub,
    Jupyter,
}

impl PySourceType {
    pub fn as_mode(&self) -> Mode {
        match self {
            PySourceType::Python | PySourceType::Stub => Mode::Module,
            PySourceType::Jupyter => Mode::Jupyter,
        }
    }

    pub const fn is_python(&self) -> bool {
        matches!(self, PySourceType::Python)
    }

    pub const fn is_stub(&self) -> bool {
        matches!(self, PySourceType::Stub)
    }

    pub const fn is_jupyter(&self) -> bool {
        matches!(self, PySourceType::Jupyter)
    }
}

impl From<&Path> for PySourceType {
    fn from(path: &Path) -> Self {
        match path.extension() {
            Some(ext) if ext == "pyi" => PySourceType::Stub,
            Some(ext) if ext == "ipynb" => PySourceType::Jupyter,
            _ => PySourceType::Python,
        }
    }
}
