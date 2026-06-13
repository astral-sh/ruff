use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Error, Expr, ExprCall, ExprMatch, Ident, ItemFn, LitStr, Pat, Path, Stmt, Token,
    parenthesized, parse::Parse, spanned::Spanned,
};

use crate::kebab_case::kebab_case;

/// A rule entry in the big match statement such a
/// `(Pycodestyle, "E112") => (RuleGroup::Preview, rules::pycodestyle::rules::logical_lines::NoIndentedBlock),`
#[derive(Clone)]
struct Rule {
    /// The actual name of the rule, e.g., `NoIndentedBlock`.
    name: Ident,
    /// The linter associated with the rule, e.g., `Pycodestyle`.
    linter: Ident,
    /// The code associated with the rule, e.g., `"E112"`.
    code: LitStr,
    /// The path to the struct implementing the rule, e.g.
    /// `rules::pycodestyle::rules::logical_lines::NoIndentedBlock`
    path: Path,
    /// The rule attributes, e.g. for feature gates
    attrs: Vec<Attribute>,
}

pub(crate) fn map_codes(func: &ItemFn) -> syn::Result<TokenStream> {
    let Some(last_stmt) = func.block.stmts.last() else {
        return Err(Error::new(
            func.block.span(),
            "expected body to end in an expression",
        ));
    };
    let Stmt::Expr(
        Expr::Call(ExprCall {
            args: some_args, ..
        }),
        _,
    ) = last_stmt
    else {
        return Err(Error::new(
            last_stmt.span(),
            "expected last expression to be `Some(match (..) { .. })`",
        ));
    };
    let mut some_args = some_args.into_iter();
    let (Some(Expr::Match(ExprMatch { arms, .. })), None) = (some_args.next(), some_args.next())
    else {
        return Err(Error::new(
            last_stmt.span(),
            "expected last expression to be `Some(match (..) { .. })`",
        ));
    };

    // Map from: linter (e.g., `Flake8Bugbear`) to rule code (e.g.,`"002"`) to rule data (e.g.,
    // `(Rule::UnaryPrefixIncrement, RuleGroup::Stable, vec![])`).
    let mut linter_to_rules: BTreeMap<Ident, BTreeMap<String, Rule>> = BTreeMap::new();

    for arm in arms {
        if matches!(arm.pat, Pat::Wild(..)) {
            break;
        }

        let rule = syn::parse::<Rule>(arm.into_token_stream().into())?;
        linter_to_rules
            .entry(rule.linter.clone())
            .or_default()
            .insert(rule.code.value(), rule);
    }

    let linter_idents: Vec<_> = linter_to_rules.keys().collect();
    let all_rules = linter_to_rules.values().flat_map(BTreeMap::values);
    let mut output = register_rules(all_rules);

    output.extend(quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum RuleCodePrefix {
            #(#linter_idents(#linter_idents),)*
        }

        impl RuleCodePrefix {
            pub fn linter(&self) -> &'static Linter {
                match self {
                    #(Self::#linter_idents(..) => &Linter::#linter_idents,)*
                }
            }

            pub fn short_code(&self) -> &'static str {
                match self {
                    #(Self::#linter_idents(code) => code.into(),)*
                }
            }
        }
    });

    for (linter, rules) in &linter_to_rules {
        output.extend(super::rule_code_prefix::expand(
            linter,
            rules
                .iter()
                .map(|(code, Rule { attrs, .. })| (code.as_str(), attrs)),
        ));

        output.extend(quote! {
            impl From<#linter> for RuleCodePrefix {
                fn from(linter: #linter) -> Self {
                    Self::#linter(linter)
                }
            }

            // Rust doesn't yet support `impl const From<RuleCodePrefix> for RuleSelector`
            // See https://github.com/rust-lang/rust/issues/67792
            impl From<#linter> for crate::rule_selector::RuleSelector {
                fn from(linter: #linter) -> Self {
                    let prefix = RuleCodePrefix::#linter(linter);
                    if is_single_rule_selector(&prefix) {
                        Self::Rule {
                            prefix,
                            redirected_from: None,
                        }
                    } else {
                        Self::Prefix {
                            prefix,
                            redirected_from: None,
                        }
                    }
                }
            }
        });
    }

    output.extend(quote! {
        impl RuleCodePrefix {
            pub(crate) fn parse(linter: &Linter, code: &str) -> Result<Self, crate::registry::FromCodeError> {
                use std::str::FromStr;

                Ok(match linter {
                    #(Linter::#linter_idents => RuleCodePrefix::#linter_idents(#linter_idents::from_str(code).map_err(|_| crate::registry::FromCodeError::Unknown)?),)*
                })
            }

        }
    });

    let rule_to_code = generate_rule_to_code(&linter_to_rules);
    output.extend(rule_to_code);

    output.extend(generate_rule_code_prefix_iter_impl(&linter_idents));

    Ok(output)
}

/// Map from rule to codes that can be used to select it.
/// This abstraction exists to support a one-to-many mapping, whereby a single rule could map
/// to multiple codes (e.g., if it existed in multiple linters, like Pylint and Flake8, under
/// different codes). We haven't actually activated this functionality yet, but some work was
/// done to support it, so the logic exists here.
fn generate_rule_to_code(linter_to_rules: &BTreeMap<Ident, BTreeMap<String, Rule>>) -> TokenStream {
    let mut rule_to_codes: HashMap<&Path, Vec<&Rule>> = HashMap::new();

    for map in linter_to_rules.values() {
        for rule in map.values() {
            let Rule { path, .. } = rule;
            rule_to_codes.entry(path).or_default().push(rule);
        }
    }

    // Keep the proc-macro output stable so unchanged code can be reused incrementally.
    for (rule, codes) in rule_to_codes
        .into_iter()
        .sorted_by_key(|(rule, _)| rule.to_token_stream().to_string())
    {
        let rule_name = rule.segments.last().unwrap();
        assert_eq!(
            codes.len(),
            1,
            "
{} is mapped to multiple codes.

The mapping of multiple codes to one rule has been disabled due to UX concerns (it would
be confusing if violations were reported under a different code than the code you selected).

We firstly want to allow rules to be selected by their names (and report them by name),
and before we can do that we have to rename all our rules to match our naming convention
(see CONTRIBUTING.md) because after that change every rule rename will be a breaking change.

See also https://github.com/astral-sh/ruff/issues/2186.
",
            rule_name.ident
        );
    }

    let rule_to_code = quote! {
        impl Linter {
            pub fn code_for_rule(&self, rule: Rule) -> Option<&'static str> {
                let metadata = rule.metadata();
                if metadata.linter == *self {
                    Some(metadata.code)
                } else {
                    None
                }
            }
        }
    };
    rule_to_code
}

/// Implement `RuleCodePrefix::iter()`
fn generate_rule_code_prefix_iter_impl(linter_idents: &[&Ident]) -> TokenStream {
    quote! {
        impl RuleCodePrefix {
            pub(crate) fn iter() -> impl Iterator<Item = RuleCodePrefix> {
                use strum::IntoEnumIterator;

                let mut prefixes = Vec::new();

                #(prefixes.extend(#linter_idents::iter().map(|x| Self::#linter_idents(x)));)*
                prefixes.into_iter()
            }
        }
    }
}

/// Generate the `Rule` enum
fn register_rules<'a>(input: impl Iterator<Item = &'a Rule>) -> TokenStream {
    let mut rule_variants = quote!();
    let mut rule_parse_match_arms = quote!();
    let mut rule_metadata = quote!();

    for Rule {
        name,
        linter,
        code,
        attrs,
        path,
    } in input
    {
        let kebab_name = kebab_case(name);
        rule_variants.extend(quote! {
            #(#attrs)*
            #name,
        });
        rule_parse_match_arms.extend(quote! {#(#attrs)* #kebab_name => Ok(Self::#name),});
        rule_metadata.extend(quote! {
            #(#attrs)*
            RuleMetadata {
                rule: Rule::#name,
                linter: Linter::#linter,
                code: #code,
                message_formats: <#path as crate::Violation>::message_formats,
                fix_availability: <#path as crate::Violation>::FIX_AVAILABILITY,
                explanation: <#path as crate::ViolationMetadata>::EXPLANATION,
                group: <#path as crate::ViolationMetadata>::GROUP,
                file: <#path as crate::ViolationMetadata>::FILE,
                line: <#path as crate::ViolationMetadata>::LINE,
            },
        });
    }

    quote! {
        #[derive(
            Debug,
            PartialEq,
            Eq,
            Copy,
            Clone,
            Hash,
            ::strum_macros::IntoStaticStr,
        )]
        #[repr(u16)]
        #[strum(serialize_all = "kebab-case")]
        pub enum Rule { #rule_variants }

        pub static RULE_METADATA: &[RuleMetadata] = &[
            #rule_metadata
        ];

        pub struct RuleIter(::std::slice::Iter<'static, RuleMetadata>);

        impl Iterator for RuleIter {
            type Item = Rule;

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next().map(|metadata| metadata.rule)
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.0.size_hint()
            }
        }

        impl ExactSizeIterator for RuleIter {}

        impl Rule {
            /// Returns the metadata for this rule.
            pub fn metadata(&self) -> &'static RuleMetadata {
                &RULE_METADATA[*self as usize]
            }

            /// Try to parse a kebab-case rule name into a `Rule`.
            pub fn from_name(name: &str) -> Result<Self, FromNameError> {
                match name {
                    #rule_parse_match_arms
                    _ => Err(FromNameError::Unknown),
                }
            }
        }
    }
}

impl Parse for Rule {
    /// Parses a match arm such as `(Pycodestyle, "E112") => rules::pycodestyle::rules::logical_lines::NoIndentedBlock,`
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = Attribute::parse_outer(input)?;
        let pat_tuple;
        parenthesized!(pat_tuple in input);
        let linter: Ident = pat_tuple.parse()?;
        let _: Token!(,) = pat_tuple.parse()?;
        let code: LitStr = pat_tuple.parse()?;
        let _: Token!(=>) = input.parse()?;
        let rule_path: Path = input.parse()?;
        let _: Token!(,) = input.parse()?;
        let rule_name = rule_path.segments.last().unwrap().ident.clone();
        Ok(Rule {
            name: rule_name,
            linter,
            code,
            path: rule_path,
            attrs,
        })
    }
}
