use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, ItemStruct, Lit, LitStr, Meta, Result};

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

/// Collect all doc comment attributes into a string
fn get_docs(attrs: &[Attribute]) -> Result<String> {
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

pub(crate) fn violation(violation: &ItemStruct) -> Result<TokenStream> {
    let ident = &violation.ident;
    let explanation = get_docs(&violation.attrs)?;
    let violation = if explanation.trim().is_empty() {
        quote! {
            #[derive(Debug, PartialEq, Eq)]
            #violation

            #[automatically_derived]
            impl From<#ident> for ruff_diagnostics::DiagnosticKind {
                fn from(value: #ident) -> Self {
                    use ruff_diagnostics::Violation;

                    Self {
                        body: Violation::message(&value),
                        suggestion: Violation::fix_title(&value),
                        name: stringify!(#ident).to_string(),
                    }
                }
            }
        }
    } else {
        quote! {
            #[derive(Debug, PartialEq, Eq)]
            #violation

            impl #ident {
                pub fn explanation() -> Option<&'static str> {
                    Some(#explanation)
                }
            }

            #[automatically_derived]
            impl From<#ident> for ruff_diagnostics::DiagnosticKind {
                fn from(value: #ident) -> Self {
                    use ruff_diagnostics::Violation;

                    Self {
                        body: Violation::message(&value),
                        suggestion: Violation::fix_title(&value),
                        name: stringify!(#ident).to_string(),
                    }
                }
            }
        }
    };
    Ok(violation)
}
