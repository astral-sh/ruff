use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Error, Expr, ExprCall, ExprMatch, Ident, ItemFn, LitStr, Pat, Path, Stmt, Token,
    parenthesized, parse::Parse, spanned::Spanned,
};

use crate::rule_code_prefix::{get_prefix_ident, intersection_all};

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
    /// The rule group identifier, e.g., `RuleGroup::Preview`.
    group: Path,
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
                .map(|(code, Rule { group, attrs, .. })| (code.as_str(), group, attrs)),
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

    let mut all_codes = Vec::new();

    for (linter, rules) in &linter_to_rules {
        let rules_by_prefix = rules_by_prefix(rules);

        for (prefix, rules) in &rules_by_prefix {
            let prefix_ident = get_prefix_ident(prefix);
            let attrs = intersection_all(rules.iter().map(|(.., attrs)| attrs.as_slice()));
            let attrs = if attrs.is_empty() {
                quote!()
            } else {
                quote!(#(#attrs)*)
            };
            all_codes.push(quote! {
                #attrs Self::#linter(#linter::#prefix_ident)
            });
        }

        let mut prefix_into_iter_match_arms = quote!();

        for (prefix, rules) in rules_by_prefix {
            let rule_paths = rules.iter().map(|(path, .., attrs)| {
                let rule_name = path.segments.last().unwrap();
                quote!(#(#attrs)* Rule::#rule_name)
            });
            let prefix_ident = get_prefix_ident(&prefix);
            let attrs = intersection_all(rules.iter().map(|(.., attrs)| attrs.as_slice()));
            let attrs = if attrs.is_empty() {
                quote!()
            } else {
                quote!(#(#attrs)*)
            };
            prefix_into_iter_match_arms.extend(quote! {
                #attrs #linter::#prefix_ident => vec![#(#rule_paths,)*].into_iter(),
            });
        }

        output.extend(quote! {
            impl #linter {
                pub fn rules(&self) -> ::std::vec::IntoIter<Rule> {
                    match self { #prefix_into_iter_match_arms }
                }
            }
        });
    }
    output.extend(quote! {
        impl RuleCodePrefix {
            pub fn parse(linter: &Linter, code: &str) -> Result<Self, crate::registry::FromCodeError> {
                use std::str::FromStr;

                Ok(match linter {
                    #(Linter::#linter_idents => RuleCodePrefix::#linter_idents(#linter_idents::from_str(code).map_err(|_| crate::registry::FromCodeError::Unknown)?),)*
                })
            }

            pub fn rules(&self) -> ::std::vec::IntoIter<Rule> {
                match self {
                    #(RuleCodePrefix::#linter_idents(prefix) => prefix.clone().rules(),)*
                }
            }
        }
    });

    let rule_to_code = generate_rule_to_code(&linter_to_rules);
    output.extend(rule_to_code);

    output.extend(generate_iter_impl(&linter_to_rules, &linter_idents));

    Ok(output)
}

/// Group the rules by their common prefixes.
fn rules_by_prefix(
    rules: &BTreeMap<String, Rule>,
) -> BTreeMap<String, Vec<(Path, Vec<Attribute>)>> {
    // TODO(charlie): Why do we do this here _and_ in `rule_code_prefix::expand`?
    let mut rules_by_prefix = BTreeMap::new();

    for code in rules.keys() {
        for i in 1..=code.len() {
            let prefix = code[..i].to_string();
            let rules: Vec<_> = rules
                .iter()
                .filter_map(|(code, rule)| {
                    if code.starts_with(&prefix) {
                        Some((rule.path.clone(), rule.attrs.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            rules_by_prefix.insert(prefix, rules);
        }
    }
    rules_by_prefix
}

/// Map from rule to codes that can be used to select it.
/// This abstraction exists to support a one-to-many mapping, whereby a single rule could map
/// to multiple codes (e.g., if it existed in multiple linters, like Pylint and Flake8, under
/// different codes). We haven't actually activated this functionality yet, but some work was
/// done to support it, so the logic exists here.
fn generate_rule_to_code(linter_to_rules: &BTreeMap<Ident, BTreeMap<String, Rule>>) -> TokenStream {
    let mut rule_to_codes: HashMap<&Path, Vec<&Rule>> = HashMap::new();
    let mut linter_code_for_rule_match_arms = quote!();

    for (linter, map) in linter_to_rules {
        for (code, rule) in map {
            let Rule {
                path, attrs, name, ..
            } = rule;
            rule_to_codes.entry(path).or_default().push(rule);
            linter_code_for_rule_match_arms.extend(quote! {
                #(#attrs)* (Self::#linter, Rule::#name) => Some(#code),
            });
        }
    }

    let mut rule_noqa_code_match_arms = quote!();
    let mut rule_group_match_arms = quote!();

    for (rule, codes) in rule_to_codes {
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

        let Rule {
            linter,
            code,
            group,
            attrs,
            ..
        } = codes
            .iter()
            .sorted_by_key(|data| data.linter == "Pylint")
            .next()
            .unwrap();

        rule_noqa_code_match_arms.extend(quote! {
            #(#attrs)* Rule::#rule_name => NoqaCode(crate::registry::Linter::#linter.common_prefix(), #code),
        });

        rule_group_match_arms.extend(quote! {
            #(#attrs)* Rule::#rule_name => #group,
        });
    }

    let rule_to_code = quote! {
        impl Rule {
            pub fn noqa_code(&self) -> NoqaCode {
                use crate::registry::RuleNamespace;

                match self {
                    #rule_noqa_code_match_arms
                }
            }

            pub fn group(&self) -> RuleGroup {
                use crate::registry::RuleNamespace;

                match self {
                    #rule_group_match_arms
                }
            }

            pub fn is_preview(&self) -> bool {
                matches!(self.group(), RuleGroup::Preview)
            }

            pub fn is_stable(&self) -> bool {
                matches!(self.group(), RuleGroup::Stable)
            }

            pub fn is_deprecated(&self) -> bool {
                matches!(self.group(), RuleGroup::Deprecated)
            }

            pub fn is_removed(&self) -> bool {
                matches!(self.group(), RuleGroup::Removed)
            }
        }

        impl Linter {
            pub fn code_for_rule(&self, rule: Rule) -> Option<&'static str> {
                match (self, rule) {
                    #linter_code_for_rule_match_arms
                    _ => None,
                }
            }
        }
    };
    rule_to_code
}

/// Implement `impl IntoIterator for &Linter` and `RuleCodePrefix::iter()`
fn generate_iter_impl(
    linter_to_rules: &BTreeMap<Ident, BTreeMap<String, Rule>>,
    linter_idents: &[&Ident],
) -> TokenStream {
    let mut linter_rules_match_arms = quote!();
    let mut linter_all_rules_match_arms = quote!();
    for (linter, map) in linter_to_rules {
        let rule_paths = map.values().map(|Rule { attrs, path, .. }| {
            let rule_name = path.segments.last().unwrap();
            quote!(#(#attrs)* Rule::#rule_name)
        });
        linter_rules_match_arms.extend(quote! {
            Linter::#linter => vec![#(#rule_paths,)*].into_iter(),
        });
        let rule_paths = map.values().map(|Rule { attrs, path, .. }| {
            let rule_name = path.segments.last().unwrap();
            quote!(#(#attrs)* Rule::#rule_name)
        });
        linter_all_rules_match_arms.extend(quote! {
            Linter::#linter => vec![#(#rule_paths,)*].into_iter(),
        });
    }

    quote! {
        impl Linter {
            /// Rules not in the preview.
            pub fn rules(self: &Linter) -> ::std::vec::IntoIter<Rule> {
                match self {
                    #linter_rules_match_arms
                }
            }
            /// All rules, including those in the preview.
            pub fn all_rules(self: &Linter) -> ::std::vec::IntoIter<Rule> {
                match self {
                    #linter_all_rules_match_arms
                }
            }
        }

        impl RuleCodePrefix {
            pub fn iter() -> impl Iterator<Item = RuleCodePrefix> {
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
    let mut rule_message_formats_match_arms = quote!();
    let mut rule_fixable_match_arms = quote!();
    let mut rule_explanation_match_arms = quote!();

    for Rule {
        name, attrs, path, ..
    } in input
    {
        rule_variants.extend(quote! {
            #(#attrs)*
            #name,
        });
        // Apply the `attrs` to each arm, like `[cfg(feature = "foo")]`.
        rule_message_formats_match_arms
            .extend(quote! {#(#attrs)* Self::#name => <#path as ruff_diagnostics::Violation>::message_formats(),});
        rule_fixable_match_arms.extend(
            quote! {#(#attrs)* Self::#name => <#path as ruff_diagnostics::Violation>::FIX_AVAILABILITY,},
        );
        rule_explanation_match_arms.extend(quote! {#(#attrs)* Self::#name => #path::explain(),});
    }

    quote! {
        use ruff_diagnostics::Violation;

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
            ::strum_macros::EnumString,
            ::serde::Serialize,
            ::serde::Deserialize,
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
                use ruff_diagnostics::ViolationMetadata;
                match self { #rule_explanation_match_arms }
            }

            /// Returns the fix status of this rule.
            pub const fn fixable(&self) -> ruff_diagnostics::FixAvailability {
                match self { #rule_fixable_match_arms }
            }
        }

        impl AsRule for ruff_diagnostics::Diagnostic {
            fn rule(&self) -> Rule {
                self.name
                    .parse()
                    .unwrap_or_else(|_| unreachable!("invalid rule name: {}", self.name))
            }
        }

    }
}

impl Parse for Rule {
    /// Parses a match arm such as `(Pycodestyle, "E112") => (RuleGroup::Preview, rules::pycodestyle::rules::logical_lines::NoIndentedBlock),`
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = Attribute::parse_outer(input)?;
        let pat_tuple;
        parenthesized!(pat_tuple in input);
        let linter: Ident = pat_tuple.parse()?;
        let _: Token!(,) = pat_tuple.parse()?;
        let code: LitStr = pat_tuple.parse()?;
        let _: Token!(=>) = input.parse()?;
        let pat_tuple;
        parenthesized!(pat_tuple in input);
        let group: Path = pat_tuple.parse()?;
        let _: Token!(,) = pat_tuple.parse()?;
        let rule_path: Path = pat_tuple.parse()?;
        let _: Token!(,) = input.parse()?;
        let rule_name = rule_path.segments.last().unwrap().ident.clone();
        Ok(Rule {
            name: rule_name,
            linter,
            code,
            group,
            path: rule_path,
            attrs,
        })
    }
}
