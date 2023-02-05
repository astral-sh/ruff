use std::collections::HashMap;

use proc_macro2::Span;
use quote::quote;
use syn::parse::Parse;
use syn::{Attribute, Ident, LitStr, Path, Token};

pub fn define_rule_mapping(mapping: &Mapping) -> proc_macro2::TokenStream {
    let mut rule_variants = quote!();
    let mut diagkind_variants = quote!();
    let mut rule_message_formats_match_arms = quote!();
    let mut rule_autofixable_match_arms = quote!();
    let mut rule_code_match_arms = quote!();
    let mut rule_from_code_match_arms = quote!();
    let mut diagkind_code_match_arms = quote!();
    let mut diagkind_body_match_arms = quote!();
    let mut diagkind_fixable_match_arms = quote!();
    let mut diagkind_commit_match_arms = quote!();
    let mut from_impls_for_diagkind = quote!();

    for (code, path, name, attr) in &mapping.entries {
        let code_str = LitStr::new(&code.to_string(), Span::call_site());
        rule_variants.extend(quote! {
            #[doc = #code_str]
            #(#attr)*
            #name,
        });
        diagkind_variants.extend(quote! {#(#attr)* #name(#path),});

        // Apply the `attrs` to each arm, like `[cfg(feature = "foo")]`.
        rule_message_formats_match_arms
            .extend(quote! {#(#attr)* Self::#name => <#path as Violation>::message_formats(),});
        rule_autofixable_match_arms
            .extend(quote! {#(#attr)* Self::#name => <#path as Violation>::AUTOFIX,});
        rule_code_match_arms.extend(quote! {#(#attr)* Self::#name => #code_str,});
        rule_from_code_match_arms.extend(quote! {#(#attr)* #code_str => Ok(Rule::#name), });
        diagkind_code_match_arms.extend(quote! {#(#attr)* Self::#name(..) => &Rule::#name, });
        diagkind_body_match_arms
            .extend(quote! {#(#attr)* Self::#name(x) => Violation::message(x), });
        diagkind_fixable_match_arms
            .extend(quote! {#(#attr)* Self::#name(x) => x.autofix_title_formatter().is_some(),});
        diagkind_commit_match_arms.extend(
            quote! {#(#attr)* Self::#name(x) => x.autofix_title_formatter().map(|f| f(x)), },
        );
        from_impls_for_diagkind.extend(quote! {
            #(#attr)*
            impl From<#path> for DiagnosticKind {
                fn from(x: #path) -> Self {
                    DiagnosticKind::#name(x)
                }
            }
        });
    }

    let code_to_name: HashMap<_, _> = mapping
        .entries
        .iter()
        .map(|(code, _, name, _)| (code.to_string(), name))
        .collect();

    let rulecodeprefix = super::rule_code_prefix::expand(
        &Ident::new("Rule", Span::call_site()),
        &Ident::new("RuleCodePrefix", Span::call_site()),
        mapping.entries.iter().map(|(code, ..)| code),
        |code| code_to_name[code],
        mapping.entries.iter().map(|(.., attr)| attr),
    );

    quote! {
        #[derive(
            EnumIter,
            Debug,
            PartialEq,
            Eq,
            Clone,
            Hash,
            PartialOrd,
            Ord,
            AsRefStr,
        )]
        #[strum(serialize_all = "kebab-case")]
        pub enum Rule { #rule_variants }

        #[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum DiagnosticKind { #diagkind_variants }

        #[derive(thiserror::Error, Debug)]
        pub enum FromCodeError {
            #[error("unknown rule code")]
            Unknown,
        }

        impl Rule {
            /// Returns the format strings used to report violations of this rule.
            pub fn message_formats(&self) -> &'static [&'static str] {
                match self { #rule_message_formats_match_arms }
            }

            pub fn autofixable(&self) -> Option<crate::violation::AutofixKind> {
                match self { #rule_autofixable_match_arms }
            }

            pub fn code(&self) -> &'static str {
                match self { #rule_code_match_arms }
            }

            pub fn from_code(code: &str) -> Result<Self, FromCodeError> {
                match code {
                    #rule_from_code_match_arms
                    _ => Err(FromCodeError::Unknown),
                }
            }
        }

        impl DiagnosticKind {
            /// The rule of the diagnostic.
            pub fn rule(&self) -> &'static Rule {
                match self { #diagkind_code_match_arms }
            }

            /// The body text for the diagnostic.
            pub fn body(&self) -> String {
                match self { #diagkind_body_match_arms }
            }

            /// Whether the diagnostic is (potentially) fixable.
            pub fn fixable(&self) -> bool {
                match self { #diagkind_fixable_match_arms }
            }

            /// The message used to describe the fix action for a given `DiagnosticKind`.
            pub fn commit(&self) -> Option<String> {
                match self { #diagkind_commit_match_arms }
            }
        }

        #from_impls_for_diagkind

        #rulecodeprefix
    }
}

pub struct Mapping {
    entries: Vec<(Ident, Path, Ident, Vec<Attribute>)>,
}

impl Parse for Mapping {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            // Grab the `#[cfg(...)]` attributes.
            let attrs = input.call(Attribute::parse_outer)?;

            // Parse the `RuleCodePrefix::... => ...` part.
            let code: Ident = input.parse()?;
            let _: Token![=>] = input.parse()?;
            let path: Path = input.parse()?;
            let name = path.segments.last().unwrap().ident.clone();
            let _: Token![,] = input.parse()?;
            entries.push((code, path, name, attrs));
        }
        Ok(Self { entries })
    }
}
