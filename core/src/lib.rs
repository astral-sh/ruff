#![doc(html_logo_url = "https://raw.githubusercontent.com/RustPython/RustPython/main/logo.png")]
#![doc(html_root_url = "https://docs.rs/rustpython-parser-core/")]

mod error;
mod format;
pub mod mode;

pub use error::BaseError;
pub use format::ConversionFlag;
pub use mode::Mode;

// re-export our public interface
pub use ruff_text_size as text_size;
