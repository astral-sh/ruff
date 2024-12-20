//! A library for formatting of text or programming code snippets.
//!
//! It's primary purpose is to build an ASCII-graphical representation of the snippet
//! with annotations.
//!
//! # Example
//!
//! ```rust
#![doc = include_str!("../examples/expected_type.rs")]
//! ```
//!
#![doc = include_str!("../examples/expected_type.svg")]
//!
//! The crate uses a three stage process with two conversions between states:
//!
//! ```text
//! Message --> Renderer --> impl Display
//! ```
//!
//! The input type - [Message] is a structure designed
//! to align with likely output from any parser whose code snippet is to be
//! annotated.
//!
//! The middle structure - [Renderer] is a structure designed
//! to convert a snippet into an internal structure that is designed to store
//! the snippet data in a way that is easy to format.
//! [Renderer] also handles the user-configurable formatting
//! options, such as color, or margins.
//!
//! Finally, `impl Display` into a final `String` output.
//!
//! # features
//! - `testing-colors` - Makes [Renderer::styled] colors OS independent, which
//! allows for easier testing when testing colored output. It should be added as
//! a feature in `[dev-dependencies]`, which can be done with the following command:
//! ```text
//! cargo add annotate-snippets --dev --feature testing-colors
//! ```

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]
#![warn(missing_debug_implementations)]

pub mod renderer;
mod snippet;

#[doc(inline)]
pub use renderer::Renderer;
pub use snippet::*;
