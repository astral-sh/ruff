use std::path::Path;

pub use expression::*;
pub use nodes::*;

pub mod all;
pub mod call_path;
pub mod comparable;
pub mod docstrings;
mod expression;
pub mod hashable;
pub mod helpers;
pub mod identifier;
pub mod imports;
pub mod node;
mod nodes;
pub mod parenthesize;
pub mod relocate;
pub mod statement_visitor;
pub mod stmt_if;
pub mod str;
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

impl From<&Path> for SourceType {
    fn from(path: &Path) -> Self {
        match path.file_name() {
            Some(filename) if filename == "pyproject.toml" => Self::Toml(TomlSourceType::Pyproject),
            Some(filename) if filename == "Pipfile" => Self::Toml(TomlSourceType::Pipfile),
            Some(filename) if filename == "poetry.lock" => Self::Toml(TomlSourceType::Poetry),
            _ => match path.extension() {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, is_macro::Is)]
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

impl From<&Path> for PySourceType {
    fn from(path: &Path) -> Self {
        match path.extension() {
            Some(ext) if ext == "py" => PySourceType::Python,
            Some(ext) if ext == "pyi" => PySourceType::Stub,
            Some(ext) if ext == "ipynb" => PySourceType::Ipynb,
            _ => PySourceType::Python,
        }
    }
}
