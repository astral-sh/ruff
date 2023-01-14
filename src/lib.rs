//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/charliermarsh/ruff
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
#![forbid(unsafe_code)]

extern crate core;

pub mod ast;
pub mod autofix;
pub mod cache;
mod checkers;
mod cst;
pub mod directives;
mod doc_lines;
mod docstrings;
mod eradicate;
pub mod fix;
mod flake8_2020;
pub mod flake8_annotations;
pub mod flake8_bandit;
mod flake8_blind_except;
pub mod flake8_boolean_trap;
pub mod flake8_bugbear;
mod flake8_builtins;
mod flake8_comprehensions;
mod flake8_datetimez;
mod flake8_debugger;
pub mod flake8_errmsg;
mod flake8_implicit_str_concat;
mod flake8_import_conventions;
pub mod flake8_pie;
mod flake8_print;
pub mod flake8_pytest_style;
pub mod flake8_quotes;
mod flake8_return;
mod flake8_simplify;
pub mod flake8_tidy_imports;
mod flake8_unused_arguments;
pub mod fs;
mod isort;
mod lex;
pub mod linter;
pub mod logging;
pub mod mccabe;
pub mod message;
mod noqa;
mod pandas_vet;
pub mod pep8_naming;
mod pycodestyle;
pub mod pydocstyle;
mod pyflakes;
mod pygrep_hooks;
mod pylint;
mod python;
mod pyupgrade;
pub mod registry;
pub mod resolver;
mod ruff;
pub mod rustpython_helpers;
pub mod settings;
pub mod source_code;
mod vendor;
mod violation;
pub mod violations;
mod visibility;

use cfg_if::cfg_if;

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
