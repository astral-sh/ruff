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

use syn::{parse_macro_input, DeriveInput};

mod check_code_prefix;
mod config;

#[proc_macro_derive(ConfigurationOptions, attributes(option, doc, option_group))]
pub fn derive_config(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    config::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(DiagnosticCodePrefix)]
pub fn derive_check_code_prefix(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    check_code_prefix::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
