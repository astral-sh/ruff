use quote::quote;
use syn::parse::Parse;
use syn::{Attribute, Ident, Path, Token};

pub fn register_rules(input: &Input) -> proc_macro2::TokenStream {
    let mut rule_variants = quote!();
    let mut rule_message_formats_match_arms = quote!();
    let mut rule_autofixable_match_arms = quote!();
    let mut rule_explanation_match_arms = quote!();

    let mut from_impls_for_diagnostic_kind = quote!();

    for (path, name, attr) in &input.entries {
        rule_variants.extend(quote! {
            #(#attr)*
            #name,
        });
        // Apply the `attrs` to each arm, like `[cfg(feature = "foo")]`.
        rule_message_formats_match_arms
            .extend(quote! {#(#attr)* Self::#name => <#path as ruff_diagnostics::Violation>::message_formats(),});
        rule_autofixable_match_arms.extend(
            quote! {#(#attr)* Self::#name => <#path as ruff_diagnostics::Violation>::AUTOFIX,},
        );
        rule_explanation_match_arms.extend(quote! {#(#attr)* Self::#name => #path::explanation(),});

        // Enable conversion from `DiagnosticKind` to `Rule`.
        from_impls_for_diagnostic_kind.extend(quote! {#(#attr)* stringify!(#name) => Rule::#name,});
    }

    quote! {
        #[derive(
            EnumIter,
            Debug,
            PartialEq,
            Eq,
            Copy,
            Clone,
            Hash,
            PartialOrd,
            Ord,
            ::ruff_macros::CacheKey,
            AsRefStr,
            ::strum_macros::IntoStaticStr,
        )]
        #[repr(u16)]
        #[strum(serialize_all = "kebab-case")]
        pub enum Rule { #rule_variants }

        impl Rule {
            /// Returns the format strings used to report violations of this rule.
            pub fn message_formats(&self) -> &'static [&'static str] {
                match self { #rule_message_formats_match_arms }
            }

            /// Returns the documentation for this rule.
            pub fn explanation(&self) -> Option<&'static str> {
                match self { #rule_explanation_match_arms }
            }

            /// Returns the autofix status of this rule.
            pub const fn autofixable(&self) -> ruff_diagnostics::AutofixKind {
                match self { #rule_autofixable_match_arms }
            }
        }

        impl AsRule for ruff_diagnostics::DiagnosticKind {
            fn rule(&self) -> Rule {
                match self.name.as_str() {
                    #from_impls_for_diagnostic_kind
                    _ => unreachable!("invalid rule name: {}", self.name),
                }
            }
        }
    }
}

pub struct Input {
    entries: Vec<(Path, Ident, Vec<Attribute>)>,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            // Grab the `#[cfg(...)]` attributes.
            let attrs = input.call(Attribute::parse_outer)?;

            let path: Path = input.parse()?;
            let name = path.segments.last().unwrap().ident.clone();
            let _: Token![,] = input.parse()?;
            entries.push((path, name, attrs));
        }
        Ok(Self { entries })
    }
}
