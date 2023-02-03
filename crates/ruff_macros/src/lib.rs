//! This crate implements internal macros for the `ruff` library.

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, ItemFn};

mod config;
mod define_violation;
mod derive_message_formats;
mod map_codes;
mod register_rules;
mod rule_code_prefix;
mod rule_namespace;

#[proc_macro_derive(ConfigurationOptions, attributes(option, doc, option_group))]
pub fn derive_config(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    config::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro]
pub fn register_rules(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mapping = parse_macro_input!(item as register_rules::Input);
    register_rules::register_rules(&mapping).into()
}

#[proc_macro]
pub fn define_violation(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let cloned = item.clone();
    let meta = parse_macro_input!(cloned as define_violation::LintMeta);
    define_violation::define_violation(&item.into(), meta).into()
}

#[proc_macro_derive(RuleNamespace, attributes(prefix))]
pub fn derive_rule_namespace(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    rule_namespace::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn map_codes(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    map_codes::map_codes(&func)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn derive_message_formats(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    derive_message_formats::derive_message_formats(&func).into()
}
