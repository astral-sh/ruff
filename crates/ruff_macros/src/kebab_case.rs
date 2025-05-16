use heck::ToKebabCase;
use proc_macro2::TokenStream;

pub(crate) fn kebab_case(input: &syn::Ident) -> TokenStream {
    let s = input.to_string();

    let kebab_case_lit = syn::LitStr::new(&s.to_kebab_case(), input.span());

    quote::quote!(#kebab_case_lit)
}
