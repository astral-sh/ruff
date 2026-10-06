//! Regular expressions for internal patterns shared by native and WebAssembly builds.
//!
//! WebAssembly uses `regex-lite` to reduce binary size; other targets use `regex`.
//! Patterns must use syntax supported by both engines. `regex-lite` uses ASCII
//! whitespace and case folding, and does not support Unicode character classes.

#[cfg(not(target_family = "wasm"))]
pub use regex::Regex;
#[cfg(target_family = "wasm")]
pub use regex_lite::Regex;
