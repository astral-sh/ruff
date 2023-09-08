use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::Span;
use quote::quote;
use syn::{Attribute, Ident, Path};

pub(crate) fn expand<'a>(
    prefix_ident: &Ident,
    variants: impl Iterator<Item = (&'a str, &'a Path, &'a Vec<Attribute>)>,
) -> proc_macro2::TokenStream {
    // Build up a map from prefix to matching RuleCodes.
    let mut prefix_to_codes: BTreeMap<String, BTreeSet<String>> = BTreeMap::default();
    let mut code_to_attributes: BTreeMap<String, &[Attribute]> = BTreeMap::default();

    for (variant, .., attr) in variants {
        let code_str = variant.to_string();
        for i in 1..=code_str.len() {
            let prefix = code_str[..i].to_string();
            prefix_to_codes
                .entry(prefix)
                .or_default()
                .insert(code_str.clone());
        }

        code_to_attributes.insert(code_str, attr);
    }

    let variant_strs: Vec<_> = prefix_to_codes.keys().collect();
    let variant_idents: Vec<_> = prefix_to_codes
        .keys()
        .map(|prefix| {
            let ident = get_prefix_ident(prefix);
            quote! {
                #ident
            }
        })
        .collect();

    let attributes: Vec<_> = prefix_to_codes
        .values()
        .map(|codes| attributes_for_prefix(codes, &code_to_attributes))
        .collect();

    quote! {
        #[derive(
            ::strum_macros::EnumIter,
            Debug,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Clone,
            Hash,
        )]
        pub enum #prefix_ident {
            #(#attributes #variant_idents,)*
        }

        impl std::str::FromStr for #prefix_ident {
            type Err = crate::registry::FromCodeError;

            fn from_str(code: &str) -> Result<Self, Self::Err> {
                match code {
                    #(#attributes #variant_strs => Ok(Self::#variant_idents),)*
                    _ => Err(crate::registry::FromCodeError::Unknown)
                }
            }
        }

        impl From<&#prefix_ident> for &'static str {
            fn from(code: &#prefix_ident) -> Self {
                match code {
                    #(#attributes #prefix_ident::#variant_idents => #variant_strs,)*
                }
            }
        }

        impl AsRef<str> for #prefix_ident {
            fn as_ref(&self) -> &str {
                match self {
                    #(#attributes Self::#variant_idents => #variant_strs,)*
                }
            }
        }
    }
}

fn attributes_for_prefix(
    codes: &BTreeSet<String>,
    attributes: &BTreeMap<String, &[Attribute]>,
) -> proc_macro2::TokenStream {
    match if_all_same(codes.iter().map(|code| attributes[code])) {
        Some(attr) => quote!(#(#attr)*),
        None => quote!(),
    }
}

/// If all values in an iterator are the same, return that value. Otherwise,
/// return `None`.
pub(crate) fn if_all_same<T: PartialEq>(iter: impl Iterator<Item = T>) -> Option<T> {
    let mut iter = iter.peekable();
    let first = iter.next()?;
    if iter.all(|x| x == first) {
        Some(first)
    } else {
        None
    }
}

/// Returns an identifier for the given prefix.
pub(crate) fn get_prefix_ident(prefix: &str) -> Ident {
    let prefix = if prefix.as_bytes()[0].is_ascii_digit() {
        // Identifiers in Rust may not start with a number.
        format!("_{prefix}")
    } else {
        prefix.to_string()
    };
    Ident::new(&prefix, Span::call_site())
}
