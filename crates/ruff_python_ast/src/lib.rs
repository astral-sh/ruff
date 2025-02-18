use std::ffi::OsStr;
use std::path::Path;

pub use expression::*;
pub use generated::*;
pub use int::*;
pub use nodes::*;
pub use operator_precedence::*;
pub use python_version::*;

pub mod comparable;
pub mod docstrings;
mod expression;
mod generated;
pub mod helpers;
pub mod identifier;
mod int;
pub mod name;
mod node;
mod nodes;
pub mod operator_precedence;
pub mod parenthesize;
mod python_version;
pub mod relocate;
pub mod script;
pub mod statement_visitor;
pub mod stmt_if;
pub mod str;
pub mod str_prefix;
pub mod traversal;
pub mod types;
pub mod visitor;
pub mod whitespace;

/// The type of a source file.
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum SourceType {
    /// The file contains Python source code.
    Python(PySourceType),
    /// The file contains TOML.
    Toml(TomlSourceType),
}

impl Default for SourceType {
    fn default() -> Self {
        Self::Python(PySourceType::Python)
    }
}

impl<P: AsRef<Path>> From<P> for SourceType {
    fn from(path: P) -> Self {
        match path.as_ref().file_name() {
            Some(filename) if filename == "pyproject.toml" => Self::Toml(TomlSourceType::Pyproject),
            Some(filename) if filename == "Pipfile" => Self::Toml(TomlSourceType::Pipfile),
            Some(filename) if filename == "poetry.lock" => Self::Toml(TomlSourceType::Poetry),
            _ => match path.as_ref().extension() {
                Some(ext) if ext == "toml" => Self::Toml(TomlSourceType::Unrecognized),
                _ => Self::Python(PySourceType::from(path)),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum TomlSourceType {
    /// The source is a `pyproject.toml`.
    Pyproject,
    /// The source is a `Pipfile`.
    Pipfile,
    /// The source is a `poetry.lock`.
    Poetry,
    /// The source is an unrecognized TOML file.
    Unrecognized,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PySourceType {
    /// The source is a Python file (`.py`).
    #[default]
    Python,
    /// The source is a Python stub file (`.pyi`).
    Stub,
    /// The source is a Jupyter notebook (`.ipynb`).
    Ipynb,
}

impl PySourceType {
    /// Infers the source type from the file extension.
    ///
    /// Falls back to `Python` if the extension is not recognized.
    pub fn from_extension(extension: &str) -> Self {
        Self::try_from_extension(extension).unwrap_or_default()
    }

    /// Infers the source type from the file extension.
    pub fn try_from_extension(extension: &str) -> Option<Self> {
        let ty = match extension {
            "py" => Self::Python,
            "pyi" => Self::Stub,
            "ipynb" => Self::Ipynb,
            _ => return None,
        };

        Some(ty)
    }

    pub fn try_from_path(path: impl AsRef<Path>) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(OsStr::to_str)
            .and_then(Self::try_from_extension)
    }

    pub const fn is_py_file(self) -> bool {
        matches!(self, Self::Python)
    }

    pub const fn is_stub(self) -> bool {
        matches!(self, Self::Stub)
    }

    pub const fn is_py_file_or_stub(self) -> bool {
        matches!(self, Self::Python | Self::Stub)
    }

    pub const fn is_ipynb(self) -> bool {
        matches!(self, Self::Ipynb)
    }
}

impl<P: AsRef<Path>> From<P> for PySourceType {
    fn from(path: P) -> Self {
        Self::try_from_path(path).unwrap_or_default()
    }
}
