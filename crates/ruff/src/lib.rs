//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/charliermarsh/ruff

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
mod visibility;

use cfg_if::cfg_if;
pub use rule_selector::RuleSelector;
pub use rules::pycodestyle::rules::IOError;
pub use violation::{AutofixKind, Availability as AutofixAvailability};

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

#[cfg(test)]
mod test;
