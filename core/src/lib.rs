#![doc(html_logo_url = "https://raw.githubusercontent.com/RustPython/RustPython/main/logo.png")]
#![doc(html_root_url = "https://docs.rs/rustpython-compiler-core/")]

mod bytecode;
mod error;
mod location;
pub mod marshal;
mod mode;

pub use bytecode::*;
pub use error::{BaseError, LocatedError};
pub use location::{Location, LocationRange};
pub use mode::Mode;

pub use ruff_text_size as text_size; // re-export mandatory and frequently accessed dependency

// FIXME: temp code
pub fn to_location(offset: &text_size::TextSize, source: &str) -> Location {
    todo!()
}
