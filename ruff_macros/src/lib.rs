//! This crate implements internal macros for the `ruff` library.
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

use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

mod config;
mod prefixes;
mod rule_code_prefix;

#[proc_macro_derive(ConfigurationOptions, attributes(option, doc, option_group))]
pub fn derive_config(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    config::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(RuleCodePrefix)]
pub fn derive_rule_code_prefix(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    rule_code_prefix::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro]
pub fn origin_by_code(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ident = parse_macro_input!(item as Ident).to_string();
    let mut iter = prefixes::PREFIX_TO_ORIGIN.iter();
    let origin = loop {
        let (prefix, origin) = iter
            .next()
            .unwrap_or_else(|| panic!("code doesn't start with any recognized prefix: {ident}"));
        if ident.starts_with(prefix) {
            break origin;
        }
    };
    let prefix = Ident::new(origin, Span::call_site());

    quote! {
        RuleOrigin::#prefix
    }
    .into()
}
