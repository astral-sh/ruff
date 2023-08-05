use ruff_text_size::{TextRange, TextSize};
use std::path::Path;

pub mod all;
pub mod call_path;
pub mod cast;
pub mod comparable;
pub mod docstrings;
pub mod hashable;
pub mod helpers;
pub mod identifier;
pub mod imports;
pub mod node;
mod nodes;
pub mod relocate;
pub mod statement_visitor;
pub mod stmt_if;
pub mod str;
pub mod traversal;
pub mod types;
pub mod visitor;
pub mod whitespace;

pub use nodes::*;

pub trait Ranged {
    fn range(&self) -> TextRange;

    fn start(&self) -> TextSize {
        self.range().start()
    }

    fn end(&self) -> TextSize {
        self.range().end()
    }
}

impl Ranged for TextRange {
    fn range(&self) -> TextRange {
        *self
    }
}

impl<T> Ranged for &T
where
    T: Ranged,
{
    fn range(&self) -> TextRange {
        T::range(self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PySourceType {
    #[default]
    Python,
    Stub,
    Jupyter,
}

impl PySourceType {
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
