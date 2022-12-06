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

pub mod generate_check_code_prefix;
pub mod generate_options;
pub mod generate_rules_table;
pub mod generate_source_code;
pub mod print_ast;
pub mod print_cst;
pub mod print_tokens;
