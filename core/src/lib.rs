#![doc(html_logo_url = "https://raw.githubusercontent.com/RustPython/RustPython/main/logo.png")]
#![doc(html_root_url = "https://docs.rs/rustpython-compiler-core/")]

// parser core
mod error;
mod mode;

pub use error::BaseError;
pub use mode::Mode;
pub use ruff_text_size as text_size; // re-export mandatory and frequently accessed dependency

// compiler core
mod bytecode;
pub mod marshal;

pub use bytecode::*;
pub use error::LocatedError;
pub use ruff_python_ast::source_code;
pub use ruff_python_ast::source_code::OneIndexed as LineNumber;

use source_code::{LineIndex, SourceCode, SourceLocation};
use text_size::TextSize;
/// Converts source code byte-offset to Python convention line and column numbers.
pub struct SourceLocator<'a> {
    pub source: &'a str,
    index: LineIndex,
}

impl<'a> SourceLocator<'a> {
    #[inline]
    pub fn new(source: &'a str) -> Self {
        let index = LineIndex::from_source_text(source);
        Self { source, index }
    }

    pub fn locate(&mut self, offset: TextSize) -> SourceLocation {
        let code = SourceCode::new(self.source, &self.index);
        let offset = unsafe { std::mem::transmute(offset) }; // temp code to fix text_size dependency
        code.source_location(offset)
    }
}
