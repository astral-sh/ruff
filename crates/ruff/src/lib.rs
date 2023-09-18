//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/astral-sh/ruff

#[cfg(feature = "clap")]
pub use rule_selector::clap_completion::RuleSelectorParser;
pub use rule_selector::RuleSelector;
pub use rules::pycodestyle::rules::{IOError, SyntaxError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod autofix;
mod checkers;
pub mod codes;
mod comments;
mod cst;
pub mod directives;
mod doc_lines;
mod docstrings;
pub mod fs;
mod importer;
mod lex;
pub mod line_width;
pub mod linter;
pub mod logging;
pub mod message;
mod noqa;
pub mod packaging;
pub mod pyproject_toml;
pub mod registry;
mod renamer;
mod rule_redirects;
pub mod rule_selector;
pub mod rules;
pub mod settings;
pub mod source_kind;
pub mod upstream_categories;

#[cfg(any(test, fuzzing))]
pub mod test;

pub const RUFF_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
