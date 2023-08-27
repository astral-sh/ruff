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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PySourceType {
    #[default]
    Python,
    Stub,
    Ipynb,
}

impl PySourceType {
    pub const fn is_python(&self) -> bool {
        matches!(self, PySourceType::Python)
    }

    pub const fn is_stub(&self) -> bool {
        matches!(self, PySourceType::Stub)
    }

    pub const fn is_ipynb(&self) -> bool {
        matches!(self, PySourceType::Ipynb)
    }
}

impl From<&Path> for PySourceType {
    fn from(path: &Path) -> Self {
        match path.extension() {
            Some(ext) if ext == "pyi" => PySourceType::Stub,
            Some(ext) if ext == "ipynb" => PySourceType::Ipynb,
            _ => PySourceType::Python,
        }
    }
}
