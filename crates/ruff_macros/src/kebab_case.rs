use proc_macro2::TokenStream;

pub(crate) fn kebab_case(input: &syn::Ident) -> TokenStream {
    let screaming_snake_case = input.to_string();

    let mut kebab_case = String::with_capacity(screaming_snake_case.len());

    for (i, word) in screaming_snake_case.split('_').enumerate() {
        if i > 0 {
            kebab_case.push('-');
        }

        kebab_case.push_str(&word.to_lowercase());
    }

    let kebab_case_lit = syn::LitStr::new(&kebab_case, input.span());

    quote::quote!(#kebab_case_lit)
}
