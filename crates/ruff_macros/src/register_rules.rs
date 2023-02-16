use quote::quote;
use syn::parse::Parse;
use syn::{Attribute, Ident, Path, Token};

pub fn register_rules(input: &Input) -> proc_macro2::TokenStream {
    let mut rule_variants = quote!();
    let mut diagnostic_kind_variants = quote!();
    let mut rule_message_formats_match_arms = quote!();
    let mut rule_autofixable_match_arms = quote!();
    let mut rule_explanation_match_arms = quote!();
    let mut diagnostic_kind_code_match_arms = quote!();
    let mut diagnostic_kind_body_match_arms = quote!();
    let mut diagnostic_kind_fixable_match_arms = quote!();
    let mut diagnostic_kind_commit_match_arms = quote!();
    let mut from_impls_for_diagnostic_kind = quote!();

    for (path, name, attr) in &input.entries {
        rule_variants.extend(quote! {
            #(#attr)*
            #name,
        });
        diagnostic_kind_variants.extend(quote! {#(#attr)* #name(#path),});

        // Apply the `attrs` to each arm, like `[cfg(feature = "foo")]`.
        rule_message_formats_match_arms
            .extend(quote! {#(#attr)* Self::#name => <#path as Violation>::message_formats(),});
        rule_autofixable_match_arms
            .extend(quote! {#(#attr)* Self::#name => <#path as Violation>::AUTOFIX,});
        rule_explanation_match_arms.extend(quote! {#(#attr)* Self::#name => #path::explanation(),});
        diagnostic_kind_code_match_arms
            .extend(quote! {#(#attr)* Self::#name(..) => &Rule::#name, });
        diagnostic_kind_body_match_arms
            .extend(quote! {#(#attr)* Self::#name(x) => Violation::message(x), });
        diagnostic_kind_fixable_match_arms
            .extend(quote! {#(#attr)* Self::#name(x) => x.autofix_title_formatter().is_some(),});
        diagnostic_kind_commit_match_arms.extend(
            quote! {#(#attr)* Self::#name(x) => x.autofix_title_formatter().map(|f| f(x)), },
        );
        from_impls_for_diagnostic_kind.extend(quote! {
            #(#attr)*
            impl From<#path> for DiagnosticKind {
                fn from(x: #path) -> Self {
                    DiagnosticKind::#name(x)
                }
            }
        });
    }

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
            ::strum_macros::IntoStaticStr,
        )]
        #[strum(serialize_all = "kebab-case")]
        pub enum Rule { #rule_variants }

        #[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum DiagnosticKind { #diagnostic_kind_variants }


        impl Rule {
            /// Returns the format strings used to report violations of this rule.
            pub fn message_formats(&self) -> &'static [&'static str] {
                match self { #rule_message_formats_match_arms }
            }

            pub fn explanation(&self) -> Option<&'static str> {
                match self { #rule_explanation_match_arms }
            }

            pub fn autofixable(&self) -> Option<crate::violation::AutofixKind> {
                match self { #rule_autofixable_match_arms }
            }
        }

        impl DiagnosticKind {
            /// The rule of the diagnostic.
            pub fn rule(&self) -> &'static Rule {
                match self { #diagnostic_kind_code_match_arms }
            }

            /// The body text for the diagnostic.
            pub fn body(&self) -> String {
                match self { #diagnostic_kind_body_match_arms }
            }

            /// Whether the diagnostic is (potentially) fixable.
            pub fn fixable(&self) -> bool {
                match self { #diagnostic_kind_fixable_match_arms }
            }

            /// The message used to describe the fix action for a given `DiagnosticKind`.
            pub fn commit(&self) -> Option<String> {
                match self { #diagnostic_kind_commit_match_arms }
            }
        }

        #from_impls_for_diagnostic_kind
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
