use std::collections::{BTreeMap, BTreeSet, HashMap};

use once_cell::sync::Lazy;
use proc_macro2::Span;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{DataEnum, DeriveInput, Ident, Variant};

const ALL: &str = "ALL";

/// A hash map from deprecated `RuleCodePrefix` to latest
/// `RuleCodePrefix`.
pub static PREFIX_REDIRECTS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", "UP001"),
        ("U003", "UP003"),
        ("U004", "UP004"),
        ("U005", "UP005"),
        ("U006", "UP006"),
        ("U007", "UP007"),
        ("U008", "UP008"),
        ("U009", "UP009"),
        ("U010", "UP010"),
        ("U011", "UP011"),
        ("U012", "UP012"),
        ("U013", "UP013"),
        ("U014", "UP014"),
        ("U015", "UP015"),
        ("U016", "UP016"),
        ("U017", "UP017"),
        ("U019", "UP019"),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", "TID252"),
        ("M001", "RUF100"),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV002", "PD002"),
        ("PDV003", "PD003"),
        ("PDV004", "PD004"),
        ("PDV007", "PD007"),
        ("PDV008", "PD008"),
        ("PDV009", "PD009"),
        ("PDV010", "PD010"),
        ("PDV011", "PD011"),
        ("PDV012", "PD012"),
        ("PDV013", "PD013"),
        ("PDV015", "PD015"),
        ("PDV901", "PD901"),
        // TODO(charlie): Remove by 2023-02-01.
        ("R501", "RET501"),
        ("R502", "RET502"),
        ("R503", "RET503"),
        ("R504", "RET504"),
        ("R505", "RET505"),
        ("R506", "RET506"),
        ("R507", "RET507"),
        ("R508", "RET508"),
        ("IC001", "ICN001"),
        ("IC002", "ICN001"),
        ("IC003", "ICN001"),
        ("IC004", "ICN001"),
        // TODO(charlie): Remove by 2023-01-01.
        ("U", "UP"),
        ("U0", "UP0"),
        ("U00", "UP00"),
        ("U01", "UP01"),
        // TODO(charlie): Remove by 2023-02-01.
        ("I2", "TID2"),
        ("I25", "TID25"),
        ("M", "RUF100"),
        ("M0", "RUF100"),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV", "PD"),
        ("PDV0", "PD0"),
        ("PDV01", "PD01"),
        ("PDV9", "PD9"),
        ("PDV90", "PD90"),
        // TODO(charlie): Remove by 2023-02-01.
        ("R", "RET"),
        ("R5", "RET5"),
        ("R50", "RET50"),
        // TODO(charlie): Remove by 2023-02-01.
        ("IC", "ICN"),
        ("IC0", "ICN0"),
    ])
});

pub fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;
    let syn::Data::Enum(DataEnum { variants, .. }) = data else {
        return Err(syn::Error::new(
            ident.span(),
            "Can only derive `RuleCodePrefix` from enums.",
        ));
    };

    let prefix_ident = Ident::new(&format!("{ident}Prefix"), ident.span());
    let prefix = expand(&ident, &prefix_ident, &variants);
    let expanded = quote! {
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        pub enum SuffixLength {
            None,
            Zero,
            One,
            Two,
            Three,
            Four,
        }

        #prefix
    };
    Ok(expanded)
}

fn expand(
    ident: &Ident,
    prefix_ident: &Ident,
    variants: &Punctuated<Variant, Comma>,
) -> proc_macro2::TokenStream {
    // Build up a map from prefix to matching RuleCodes.
    let mut prefix_to_codes: BTreeMap<Ident, BTreeSet<String>> = BTreeMap::default();
    for variant in variants {
        let span = variant.ident.span();
        let code_str = variant.ident.to_string();
        let code_prefix_len = code_str
            .chars()
            .take_while(|char| char.is_alphabetic())
            .count();
        let code_suffix_len = code_str.len() - code_prefix_len;
        for i in 0..=code_suffix_len {
            let prefix = code_str[..code_prefix_len + i].to_string();
            prefix_to_codes
                .entry(Ident::new(&prefix, span))
                .or_default()
                .insert(code_str.clone());
        }
        prefix_to_codes
            .entry(Ident::new(ALL, span))
            .or_default()
            .insert(code_str.clone());
    }

    // Add any prefix aliases (e.g., "U" to "UP").
    for (alias, rule_code) in PREFIX_REDIRECTS.iter() {
        prefix_to_codes.insert(
            Ident::new(alias, Span::call_site()),
            prefix_to_codes
                .get(&Ident::new(rule_code, Span::call_site()))
                .unwrap_or_else(|| panic!("Unknown RuleCode: {alias:?}"))
                .clone(),
        );
    }

    let prefix_variants = prefix_to_codes.keys().map(|prefix| {
        quote! {
            #prefix
        }
    });

    let prefix_impl = generate_impls(ident, prefix_ident, &prefix_to_codes);

    let prefix_redirects = PREFIX_REDIRECTS.iter().map(|(alias, rule_code)| {
        let code = Ident::new(rule_code, Span::call_site());
        quote! {
            (#alias, #prefix_ident::#code)
        }
    });

    quote! {
        #[derive(
            ::strum_macros::EnumString,
            ::strum_macros::AsRefStr,
            Debug,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Clone,
            ::serde::Serialize,
            ::serde::Deserialize,
            ::schemars::JsonSchema,
        )]
        pub enum #prefix_ident {
            #(#prefix_variants,)*
        }

        #prefix_impl

        /// A hash map from deprecated `RuleCodePrefix` to latest `RuleCodePrefix`.
        pub static PREFIX_REDIRECTS: ::once_cell::sync::Lazy<::rustc_hash::FxHashMap<&'static str, #prefix_ident>> = ::once_cell::sync::Lazy::new(|| {
            ::rustc_hash::FxHashMap::from_iter([
                #(#prefix_redirects),*
            ])
        });
    }
}

fn generate_impls(
    ident: &Ident,
    prefix_ident: &Ident,
    prefix_to_codes: &BTreeMap<Ident, BTreeSet<String>>,
) -> proc_macro2::TokenStream {
    let codes_match_arms = prefix_to_codes.iter().map(|(prefix, codes)| {
        let codes = codes.iter().map(|code| {
            let code = Ident::new(code, Span::call_site());
            quote! {
                #ident::#code
            }
        });
        let prefix_str = prefix.to_string();
        if let Some(target) = PREFIX_REDIRECTS.get(prefix_str.as_str()) {
            quote! {
                #prefix_ident::#prefix => {
                    crate::warn_user_once!(
                        "`{}` has been remapped to `{}`", #prefix_str, #target
                    );
                    vec![#(#codes),*]
                }
            }
        } else {
            quote! {
                #prefix_ident::#prefix => vec![#(#codes),*],
            }
        }
    });

    let specificity_match_arms = prefix_to_codes.keys().map(|prefix| {
        if *prefix == ALL {
            quote! {
                #prefix_ident::#prefix => SuffixLength::None,
            }
        } else {
            let num_numeric = prefix
                .to_string()
                .chars()
                .filter(|char| char.is_numeric())
                .count();
            let suffix_len = match num_numeric {
                0 => quote! { SuffixLength::Zero },
                1 => quote! { SuffixLength::One },
                2 => quote! { SuffixLength::Two },
                3 => quote! { SuffixLength::Three },
                4 => quote! { SuffixLength::Four },
                _ => panic!("Invalid prefix: {prefix}"),
            };
            quote! {
                #prefix_ident::#prefix => #suffix_len,
            }
        }
    });

    let categories = prefix_to_codes.keys().map(|prefix| {
        let prefix_str = prefix.to_string();
        if prefix_str.chars().all(char::is_alphabetic)
            && !PREFIX_REDIRECTS.contains_key(&prefix_str.as_str())
        {
            quote! {
                #prefix_ident::#prefix,
            }
        } else {
            quote! {}
        }
    });

    quote! {
        impl #prefix_ident {
            pub fn codes(&self) -> Vec<#ident> {
                use colored::Colorize;

                #[allow(clippy::match_same_arms)]
                match self {
                    #(#codes_match_arms)*
                }
            }

            pub fn specificity(&self) -> SuffixLength {
                #[allow(clippy::match_same_arms)]
                match self {
                    #(#specificity_match_arms)*
                }
            }
        }

        pub const CATEGORIES: &[#prefix_ident] = &[#(#categories)*];
    }
}
