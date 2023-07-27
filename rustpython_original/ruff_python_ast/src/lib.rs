//! Python AST node definitions and utilities.
//!
//! AST nodes are very similary defined like [Python AST](https://docs.python.org/3/library/ast.html).
//! But a few exceptions exist due to ruff_python_parser optimization.
//! They can be transformed to matching Python-styled AST in reasonable cost.
//!
//! [PythonArguments] is replaced by [Arguments]. The new [Arguments] type representation uses a new type
//! [ArgWithDefault] to represent arguments with default values. See each type documentation for more details.
//!
//! A few top-level sum types are renamed to human friendly names.
//! [CmpOp] refers `cmpop`
//! [UnaryOp] refers `unaryop`
//! [BoolOp] refers `boolop`
//! [WithItem] refers `withitem`
//! [ExceptHandler] refers `excepthandler`
//!

mod nodes;

pub use nodes::*;
use ruff_text_size::{TextRange, TextSize};

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
