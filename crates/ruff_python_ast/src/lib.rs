use ruff_text_size::{TextRange, TextSize};

pub mod all;
pub mod call_path;
pub mod cast;
pub mod comparable;
pub mod docstrings;
pub mod function;
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
