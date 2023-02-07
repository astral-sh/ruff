use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::quote;
use syn::{Attribute, Ident};

pub fn expand<'a>(
    rule_type: &Ident,
    prefix_ident: &Ident,
    variants: impl Iterator<Item = &'a Ident>,
    variant_name: impl Fn(&str) -> &'a Ident,
    attr: impl Iterator<Item = &'a Vec<Attribute>>,
) -> proc_macro2::TokenStream {
    // Build up a map from prefix to matching RuleCodes.
    let mut prefix_to_codes: BTreeMap<String, BTreeMap<String, Vec<Attribute>>> =
        BTreeMap::default();

    let mut pl_codes = BTreeMap::new();

    for (variant, attr) in variants.zip(attr) {
        let code_str = variant.to_string();
        let code_prefix_len = code_str
            .chars()
            .take_while(|char| char.is_alphabetic())
            .count();
        let code_suffix_len = code_str.len() - code_prefix_len;
        for i in 0..=code_suffix_len {
            let prefix = code_str[..code_prefix_len + i].to_string();
            prefix_to_codes
                .entry(prefix)
                .or_default()
                .entry(code_str.clone())
                .or_insert_with(|| attr.clone());
        }
        if code_str.starts_with("PL") {
            pl_codes.insert(code_str, attr.clone());
        }
    }

    prefix_to_codes.insert("PL".to_string(), pl_codes);

    let prefix_variants = prefix_to_codes.iter().map(|(prefix, codes)| {
        let prefix = Ident::new(prefix, Span::call_site());
        let attr = if_all_same(codes.values().cloned()).unwrap_or_default();
        quote! {
            #(#attr)*
            #prefix
        }
    });

    let prefix_impl = generate_impls(rule_type, prefix_ident, &prefix_to_codes, variant_name);

    quote! {
        #[derive(
            ::strum_macros::EnumIter,
            ::strum_macros::EnumString,
            ::strum_macros::AsRefStr,
            ::strum_macros::IntoStaticStr,
            Debug,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Clone,
            Hash,
            ::serde::Serialize,
            ::serde::Deserialize,
        )]
        pub enum #prefix_ident {
            #(#prefix_variants,)*
        }

        #prefix_impl
    }
}

fn generate_impls<'a>(
    rule_type: &Ident,
    prefix_ident: &Ident,
    prefix_to_codes: &BTreeMap<String, BTreeMap<String, Vec<Attribute>>>,
    variant_name: impl Fn(&str) -> &'a Ident,
) -> proc_macro2::TokenStream {
    let into_iter_match_arms = prefix_to_codes.iter().map(|(prefix_str, codes)| {
        let prefix = Ident::new(prefix_str, Span::call_site());
        let attr = if_all_same(codes.values().cloned()).unwrap_or_default();
        let codes = codes.iter().map(|(code, attr)| {
            let rule_variant = variant_name(code);
            quote! {
                #(#attr)*
                #rule_type::#rule_variant
            }
        });
        quote! {
            #(#attr)*
            #prefix_ident::#prefix => vec![#(#codes),*].into_iter(),
        }
    });

    let specificity_match_arms = prefix_to_codes.iter().map(|(prefix_str, codes)| {
        let prefix = Ident::new(prefix_str, Span::call_site());
        let mut num_numeric = prefix_str.chars().filter(|char| char.is_numeric()).count();
        if prefix_str != "PL" && prefix_str.starts_with("PL") {
            num_numeric += 1;
        }
        let suffix_len = match num_numeric {
            0 => quote! { Specificity::Linter },
            1 => quote! { Specificity::Code1Char },
            2 => quote! { Specificity::Code2Chars },
            3 => quote! { Specificity::Code3Chars },
            4 => quote! { Specificity::Code4Chars },
            5 => quote! { Specificity::Code5Chars },
            _ => panic!("Invalid prefix: {prefix}"),
        };
        let attr = if_all_same(codes.values().cloned()).unwrap_or_default();
        quote! {
            #(#attr)*
            #prefix_ident::#prefix => #suffix_len,
        }
    });

    quote! {
        impl #prefix_ident {
            pub(crate) fn specificity(&self) -> crate::rule_selector::Specificity {
                use crate::rule_selector::Specificity;

                #[allow(clippy::match_same_arms)]
                match self {
                    #(#specificity_match_arms)*
                }
            }
        }

        impl IntoIterator for &#prefix_ident {
            type Item = #rule_type;
            type IntoIter = ::std::vec::IntoIter<Self::Item>;

            fn into_iter(self) -> Self::IntoIter {
                #[allow(clippy::match_same_arms)]
                match self {
                    #(#into_iter_match_arms)*
                }
            }
        }
    }
}

/// If all values in an iterator are the same, return that value. Otherwise,
/// return `None`.
fn if_all_same<T: PartialEq>(iter: impl Iterator<Item = T>) -> Option<T> {
    let mut iter = iter.peekable();
    let first = iter.next()?;
    if iter.all(|x| x == first) {
        Some(first)
    } else {
        None
    }
}
