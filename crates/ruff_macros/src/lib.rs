//! This crate implements internal macros for the `ruff` library.

use crate::cache_key::derive_cache_key;
use crate::newtype_index::generate_newtype_index;
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, ItemFn, ItemStruct};

mod cache_key;
mod combine_options;
mod config;
mod derive_message_formats;
mod map_codes;
mod newtype_index;
mod rule_code_prefix;
mod rule_namespace;
mod violation;

#[proc_macro_derive(OptionsMetadata, attributes(option, doc, option_group))]
pub fn derive_options_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    config::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(CombineOptions)]
pub fn derive_combine_options(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    combine_options::derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Generates a [`CacheKey`] implementation for the attributed type.
///
/// Struct fields can be attributed with the `cache_key` field-attribute that supports:
/// * `ignore`: Ignore the attributed field in the cache key
#[proc_macro_derive(CacheKey, attributes(cache_key))]
pub fn cache_key(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as DeriveInput);

    let result = derive_cache_key(&item);
    let stream = result.unwrap_or_else(|err| err.to_compile_error());

    TokenStream::from(stream)
}

/// Adds an `explanation()` method from the doc comment.
#[proc_macro_attribute]
pub fn violation(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let violation = parse_macro_input!(item as ItemStruct);
    violation::violation(&violation)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(RuleNamespace, attributes(prefix))]
pub fn derive_rule_namespace(input: TokenStream) -> TokenStream {
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

/// Derives a newtype wrapper that can be used as an index.
/// The wrapper can represent indices up to `u32::MAX - 1`.
///
/// The `u32::MAX - 1` is an optimization so that `Option<Index>` has the same size as `Index`.
///
/// Can store at most `u32::MAX - 1` values
///
/// ## Warning
///
/// Additional `derive` attributes must come AFTER this attribute:
///
/// Good:
///
/// ```ignore
/// use ruff_macros::newtype_index;
///
/// #[newtype_index]
/// #[derive(Ord, PartialOrd)]
/// struct MyIndex;
/// ```
#[proc_macro_attribute]
pub fn newtype_index(_metadata: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ItemStruct);

    let output = match generate_newtype_index(item) {
        Ok(output) => output,
        Err(err) => err.to_compile_error(),
    };

    TokenStream::from(output)
}
