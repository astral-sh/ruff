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
// Since this is a vendored copy of `annotate-snippets`, we squash Clippy
// warnings from upstream in order to the reduce the diff. If our copy drifts
// far from upstream such that patches become impractical to apply in both
// places, then we can get rid of these suppressions and fix the lints.
#![allow(
    clippy::return_self_not_must_use,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::explicit_iter_loop,
    clippy::unused_self,
    clippy::unnecessary_wraps,
    clippy::range_plus_one,
    clippy::redundant_closure_for_method_calls,
    clippy::struct_field_names,
    clippy::cloned_instead_of_copied,
    clippy::cast_sign_loss,
    clippy::needless_as_bytes,
    clippy::unnecessary_map_or
)]

pub mod renderer;
mod snippet;

#[doc(inline)]
pub use renderer::Renderer;
pub use snippet::*;
