//! This crate implements internal macros for the `ruff` library.

use syn::{parse_macro_input, DeriveInput, ItemFn};

mod config;
mod define_rule_mapping;
mod define_violation;
mod derive_message_formats;
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
pub fn define_rule_mapping(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mapping = parse_macro_input!(item as define_rule_mapping::Mapping);
    define_rule_mapping::define_rule_mapping(&mapping).into()
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
pub fn derive_message_formats(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    derive_message_formats::derive_message_formats(&func).into()
}
