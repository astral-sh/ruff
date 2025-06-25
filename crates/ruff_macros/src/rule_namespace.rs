use std::cmp::Reverse;
use std::collections::HashSet;

use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataEnum, DeriveInput, Error, ExprLit, Lit, Meta, MetaNameValue};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput {
        ident,
        data: Data::Enum(DataEnum { variants, .. }),
        ..
    } = input
    else {
        return Err(Error::new(
            input.ident.span(),
            "only named fields are supported",
        ));
    };

    let mut parsed = Vec::new();

    let mut common_prefix_match_arms = quote!();
    let mut name_match_arms =
        quote!(Self::Ruff => "Ruff-specific rules", Self::Numpy => "NumPy-specific rules", );
    let mut url_match_arms = quote!(Self::Ruff => None, Self::Numpy => None, );

    let mut all_prefixes = HashSet::new();

    for variant in variants {
        let mut first_chars = HashSet::new();
        let prefixes: Result<Vec<_>, _> = variant
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("prefix"))
            .map(|attr| {
                let Meta::NameValue(MetaNameValue{value: syn::Expr::Lit (ExprLit { lit: Lit::Str(lit), ..}), ..}) = &attr.meta else {
                    return Err(Error::new(attr.span(), r#"expected attribute to be in the form of [#prefix = "..."]"#));
                };
                let str = lit.value();
                match str.chars().next() {
                    None => return Err(Error::new(lit.span(), "expected prefix string to be non-empty")),
                    Some(c) => if !first_chars.insert(c) {
                        return Err(Error::new(lit.span(), format!("this variant already has another prefix starting with the character '{c}'")))
                    }
                }
                if !all_prefixes.insert(str.clone()) {
                    return Err(Error::new(lit.span(), "prefix has already been defined before"));
                }
                Ok(str)
            })
            .collect();
        let prefixes = prefixes?;

        if prefixes.is_empty() {
            return Err(Error::new(
                variant.span(),
                r#"Missing #[prefix = "..."] attribute"#,
            ));
        }

        let Some(doc_attr) = variant
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("doc"))
        else {
            return Err(Error::new(variant.span(), "expected a doc comment"));
        };

        let variant_ident = variant.ident;

        if variant_ident != "Ruff" && variant_ident != "Numpy" {
            let (name, url) = parse_doc_attr(doc_attr)?;
            name_match_arms.extend(quote! {Self::#variant_ident => #name,});
            url_match_arms.extend(quote! {Self::#variant_ident => Some(#url),});
        }

        for lit in &prefixes {
            parsed.push((
                lit.clone(),
                variant_ident.clone(),
                match prefixes.len() {
                    1 => ParseStrategy::SinglePrefix,
                    _ => ParseStrategy::MultiplePrefixes,
                },
            ));
        }

        if let [prefix] = &prefixes[..] {
            common_prefix_match_arms.extend(quote! { Self::#variant_ident => #prefix, });
        } else {
            // There is more than one prefix. We already previously asserted
            // that prefixes of the same variant don't start with the same character
            // so the common prefix for this variant is the empty string.
            common_prefix_match_arms.extend(quote! { Self::#variant_ident => "", });
        }
    }

    parsed.sort_by_key(|(prefix, ..)| Reverse(prefix.len()));

    let mut if_statements = quote!();

    for (prefix, field, strategy) in parsed {
        let ret_str = match strategy {
            ParseStrategy::SinglePrefix => quote!(rest),
            ParseStrategy::MultiplePrefixes => quote!(code),
        };
        if_statements.extend(quote! {if let Some(rest) = code.strip_prefix(#prefix) {
            return Some((#ident::#field, #ret_str));
        }});
    }

    Ok(quote! {
        #[automatically_derived]
        impl crate::registry::RuleNamespace for #ident {
            fn parse_code(code: &str) -> Option<(Self, &str)> {
                #if_statements
                None
            }

            fn common_prefix(&self) -> &'static str {
                match self { #common_prefix_match_arms }
            }

            fn name(&self) -> &'static str {
                match self { #name_match_arms }
            }

            fn url(&self) -> Option<&'static str> {
                match self { #url_match_arms }
            }
        }
    })
}

/// Parses an attribute in the form of `#[doc = " [name](https://example.com/)"]`
/// into a tuple of link label and URL.
fn parse_doc_attr(doc_attr: &Attribute) -> syn::Result<(String, String)> {
    let Meta::NameValue(MetaNameValue {
        value:
            syn::Expr::Lit(ExprLit {
                lit: Lit::Str(doc_lit),
                ..
            }),
        ..
    }) = &doc_attr.meta
    else {
        return Err(Error::new(
            doc_attr.span(),
            r#"expected doc attribute to be in the form of #[doc = "..."]"#,
        ));
    };
    parse_markdown_link(doc_lit.value().trim())
        .map(|(name, url)| (name.to_string(), url.to_string()))
        .ok_or_else(|| {
            Error::new(
                doc_lit.span(),
                "expected doc comment to be in the form of `/// [name](https://example.com/)`",
            )
        })
}

fn parse_markdown_link(link: &str) -> Option<(&str, &str)> {
    link.strip_prefix('[')?.strip_suffix(')')?.split_once("](")
}

enum ParseStrategy {
    SinglePrefix,
    MultiplePrefixes,
}
