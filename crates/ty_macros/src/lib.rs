//! This crate implements internal macros for the `ty` library.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod env_vars;

/// Generates metadata for environment variables declared in the impl block.
///
/// This attribute macro should be applied to an `impl EnvVars` block.
/// It will generate a `metadata()` method that returns all non-hidden
/// environment variables with their documentation.
#[proc_macro_attribute]
pub fn attribute_env_vars_metadata(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::ItemImpl);

    env_vars::attribute_env_vars_metadata(input).into()
}
