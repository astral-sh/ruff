use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DeriveInput, Error, Lit, LitStr, Meta};

pub(crate) fn violation_metadata(input: DeriveInput) -> syn::Result<TokenStream> {
    let docs = get_docs(&input.attrs)?;

    let name = input.ident;

    Ok(quote! {
        #[automatically_derived]
        #[allow(deprecated)]
        impl ruff_diagnostics::ViolationMetadata for #name {
            fn rule_name() -> &'static str {
                stringify!(#name)
            }

            fn explain() -> Option<&'static str> {
                Some(#docs)
            }
        }
    })
}

/// Collect all doc comment attributes into a string
fn get_docs(attrs: &[Attribute]) -> syn::Result<String> {
    let mut explanation = String::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Some(lit) = parse_attr(["doc"], attr) {
                let value = lit.value();
                // `/// ` adds
                let line = value.strip_prefix(' ').unwrap_or(&value);
                explanation.push_str(line);
                explanation.push('\n');
            } else {
                return Err(Error::new_spanned(attr, "unimplemented doc comment style"));
            }
        }
    }
    Ok(explanation)
}

fn parse_attr<'a, const LEN: usize>(
    path: [&'static str; LEN],
    attr: &'a Attribute,
) -> Option<&'a LitStr> {
    if let Meta::NameValue(name_value) = &attr.meta {
        let path_idents = name_value
            .path
            .segments
            .iter()
            .map(|segment| &segment.ident);

        if itertools::equal(path_idents, path) {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(lit), ..
            }) = &name_value.value
            {
                return Some(lit);
            }
        }
    }

    None
}
