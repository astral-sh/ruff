//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/charliermarsh/ruff
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod assert_yaml_snapshot;
mod ast;
mod autofix;
pub mod cache;
mod checkers;
mod cst;
mod directives;
mod doc_lines;
mod docstrings;
pub mod fix;
pub mod flake8_to_ruff;
pub mod fs;
mod lex;
pub mod linter;
pub mod logging;
pub mod message;
mod noqa;
mod python;
pub mod registry;
pub mod resolver;
mod rule_redirects;
mod rule_selector;
mod rules;
mod rustpython_helpers;
pub mod settings;
pub mod source_code;
mod vendor;
mod violation;
mod violations;
mod visibility;

use cfg_if::cfg_if;
pub use rule_selector::RuleSelector;
pub use violation::{AutofixKind, Availability as AutofixAvailability};
pub use violations::IOError;

cfg_if! {
    if #[cfg(not(target_family = "wasm"))] {
        pub mod packaging;


        mod lib_native;
        pub use lib_native::check;
    } else {
        mod lib_wasm;
        pub use lib_wasm::check;
    }
}
