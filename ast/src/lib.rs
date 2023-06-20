//! Python AST node definitions and utilities.
//!
//! AST nodes are very similary defined like [Python AST](https://docs.python.org/3/library/ast.html).
//! But a few exceptions exist due to parser optimization.
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

mod builtin;
mod generic;
mod impls;
mod ranged;

#[cfg(feature = "malachite-bigint")]
pub use malachite_bigint as bigint;
#[cfg(all(feature = "num-bigint", not(feature = "malachite-bigint")))]
pub use num_bigint as bigint;

pub use builtin::*;
pub use generic::*;
pub use ranged::Ranged;
pub use rustpython_parser_core::{text_size, ConversionFlag};

pub trait Node {
    const NAME: &'static str;
    const FIELD_NAMES: &'static [&'static str];
}
