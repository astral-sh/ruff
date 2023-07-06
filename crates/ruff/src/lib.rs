//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/astral-sh/ruff

pub use ruff_python_ast::source_code::round_trip;
pub use rule_selector::RuleSelector;
pub use rules::pycodestyle::rules::IOError;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod autofix;
mod checkers;
mod codes;
mod cst;
pub mod directives;
mod doc_lines;
mod docstrings;
pub mod flake8_to_ruff;
pub mod fs;
mod importer;
pub mod jupyter;
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
pub mod resolver;
mod rule_redirects;
mod rule_selector;
pub mod rules;
pub mod settings;
pub mod source_kind;

#[cfg(any(test, fuzzing))]
pub mod test;
