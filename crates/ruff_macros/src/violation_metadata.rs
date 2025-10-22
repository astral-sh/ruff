use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DeriveInput, Error, Lit, LitStr, Meta};

pub(crate) fn violation_metadata(input: DeriveInput) -> syn::Result<TokenStream> {
    let docs = get_docs(&input.attrs)?;

    let version = match get_version(&input.attrs)? {
        Some(version) => quote!(Some(#version)),
        None => quote!(None),
    };

    let name = input.ident;

    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        #[expect(deprecated)]
        impl #impl_generics crate::ViolationMetadata for #name #ty_generics #where_clause {
            fn rule() -> crate::registry::Rule {
                crate::registry::Rule::#name
            }

            fn explain() -> Option<&'static str> {
                Some(#docs)
            }

            fn version() -> Option<&'static str> {
                #version
            }

            fn file() -> &'static str {
                file!()
            }

            fn line() -> u32 {
                line!()
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

/// Extract the version attribute as a string.
fn get_version(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut version = None;
    for attr in attrs {
        if attr.path().is_ident("violation_metadata") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("version") {
                    let lit: LitStr = meta.value()?.parse()?;
                    version = Some(lit.value());
                    return Ok(());
                }
                Err(Error::new_spanned(
                    attr,
                    "unimplemented violation metadata option",
                ))
            })?;
        }
    }
    Ok(version)
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
