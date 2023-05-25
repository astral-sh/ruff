use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parenthesized, parse::Parse, spanned::Spanned, Attribute, Error, Expr, ExprCall, ExprMatch,
    Ident, ItemFn, LitStr, Pat, Path, Stmt, Token,
};

use crate::rule_code_prefix::{get_prefix_ident, if_all_same, is_nursery};

struct LinterToRuleData {
    /// The rule identifier, e.g., `Rule::UnaryPrefixIncrement`.
    rule_id: Path,
    /// The rule group identifiers, e.g., `RuleGroup::Unspecified`.
    rule_group_id: Path,
    /// The rule attributes.
    attrs: Vec<Attribute>,
}

struct RuleToLinterData<'a> {
    /// The linter associated with the rule, e.g., `Flake8Bugbear`.
    linter: &'a Ident,
    /// The code associated with the rule, e.g., `"002"`.
    code: &'a str,
    /// The rule group identifier, e.g., `RuleGroup::Unspecified`.
    rule_group_id: &'a Path,
    /// The rule attributes.
    attrs: &'a [Attribute],
}

pub(crate) fn map_codes(func: &ItemFn) -> syn::Result<TokenStream> {
    let Some(last_stmt) = func.block.stmts.last() else {
        return Err(Error::new(func.block.span(), "expected body to end in an expression"));
    };
    let Stmt::Expr(Expr::Call(ExprCall{args: some_args, ..}), _) = last_stmt else {
        return Err(Error::new(last_stmt.span(), "expected last expression to be `Some(match (..) { .. })`"))
    };
    let mut some_args = some_args.into_iter();
    let (Some(Expr::Match(ExprMatch { arms, .. })), None) = (some_args.next(), some_args.next()) else {
        return Err(Error::new(last_stmt.span(), "expected last expression to be `Some(match (..) { .. })`"))
    };

    // Map from: linter (e.g., `Flake8Bugbear`) to rule code (e.g.,`"002"`) to rule data (e.g.,
    // `(Rule::UnaryPrefixIncrement, RuleGroup::Unspecified, vec![])`).
    let mut linter_to_rules: BTreeMap<Ident, BTreeMap<String, LinterToRuleData>> = BTreeMap::new();

    for arm in arms {
        if matches!(arm.pat, Pat::Wild(..)) {
            break;
        }

        let entry = syn::parse::<Entry>(arm.into_token_stream().into())?;
        linter_to_rules.entry(entry.linter).or_default().insert(
            entry.code.value(),
            LinterToRuleData {
                rule_id: entry.rule,
                rule_group_id: entry.group,
                attrs: entry.attrs,
            },
        );
    }

    let linter_idents: Vec<_> = linter_to_rules.keys().collect();

    let mut output = quote! {
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
    };

    for (linter, rules) in &linter_to_rules {
        output.extend(super::rule_code_prefix::expand(
            linter,
            rules.iter().map(
                |(
                    code,
                    LinterToRuleData {
                        rule_group_id,
                        attrs,
                        ..
                    },
                )| (code.as_str(), rule_group_id, attrs),
            ),
        ));

        output.extend(quote! {
            impl From<#linter> for RuleCodePrefix {
                fn from(linter: #linter) -> Self {
                    Self::#linter(linter)
                }
            }
            impl From<#linter> for crate::rule_selector::RuleSelector {
                fn from(linter: #linter) -> Self {
                    Self::Prefix{prefix: RuleCodePrefix::#linter(linter), redirected_from: None}
                }
            }
        });
    }

    let mut all_codes = Vec::new();

    for (linter, rules) in &linter_to_rules {
        let rules_by_prefix = rules_by_prefix(rules);

        for (prefix, rules) in &rules_by_prefix {
            let prefix_ident = get_prefix_ident(prefix);
            let attr = match if_all_same(rules.iter().map(|(.., attrs)| attrs)) {
                Some(attr) => quote!(#(#attr)*),
                None => quote!(),
            };
            all_codes.push(quote! {
                #attr Self::#linter(#linter::#prefix_ident)
            });
        }

        let mut prefix_into_iter_match_arms = quote!();

        for (prefix, rules) in rules_by_prefix {
            let rule_paths = rules.iter().map(|(path, .., attrs)| {
                let rule_name = path.segments.last().unwrap();
                quote!(#(#attrs)* Rule::#rule_name)
            });
            let prefix_ident = get_prefix_ident(&prefix);
            let attr = match if_all_same(rules.iter().map(|(.., attrs)| attrs)) {
                Some(attr) => quote!(#(#attr)*),
                None => quote!(),
            };
            prefix_into_iter_match_arms.extend(quote! {
                #attr #linter::#prefix_ident => vec![#(#rule_paths,)*].into_iter(),
            });
        }

        output.extend(quote! {
            impl IntoIterator for &#linter {
                type Item = Rule;
                type IntoIter = ::std::vec::IntoIter<Self::Item>;

                fn into_iter(self) -> Self::IntoIter {
                    match self { #prefix_into_iter_match_arms }
                }
            }
        });
    }

    output.extend(quote! {
        impl IntoIterator for &RuleCodePrefix {
            type Item = Rule;
            type IntoIter = ::std::vec::IntoIter<Self::Item>;

            fn into_iter(self) -> Self::IntoIter {
                match self {
                    #(RuleCodePrefix::#linter_idents(prefix) => prefix.into_iter(),)*
                }
            }
        }
    });

    output.extend(quote! {
        impl RuleCodePrefix {
            pub fn parse(linter: &Linter, code: &str) -> Result<Self, crate::registry::FromCodeError> {
                use std::str::FromStr;

                Ok(match linter {
                    #(Linter::#linter_idents => RuleCodePrefix::#linter_idents(#linter_idents::from_str(code).map_err(|_| crate::registry::FromCodeError::Unknown)?),)*
                })
            }
        }
    });

    let rule_to_code = generate_rule_to_code(&mut linter_to_rules);
    output.extend(rule_to_code);

    let iter = generate_iter_impl(&mut linter_to_rules, &mut all_codes);
    output.extend(iter);

    Ok(output)
}

/// Group the rules by their common prefixes.
fn rules_by_prefix(
    rules: &BTreeMap<String, LinterToRuleData>,
) -> BTreeMap<String, Vec<(Path, Vec<Attribute>)>> {
    // TODO(charlie): Why do we do this here _and_ in `rule_code_prefix::expand`?
    let mut rules_by_prefix = BTreeMap::new();

    for (
        code,
        LinterToRuleData {
            rule_id,
            rule_group_id,
            attrs,
        },
    ) in rules
    {
        // Nursery rules have to be explicitly selected, so we ignore them when looking at
        // prefixes.
        if is_nursery(rule_group_id) {
            rules_by_prefix.insert(code.clone(), vec![(rule_id.clone(), attrs.clone())]);
            continue;
        }

        for i in 1..=code.len() {
            let prefix = code[..i].to_string();
            let rules: Vec<_> = rules
                .iter()
                .filter_map(
                    |(
                        code,
                        LinterToRuleData {
                            rule_id,
                            rule_group_id,
                            attrs,
                        },
                    )| {
                        // Nursery rules have to be explicitly selected, so we ignore them when
                        // looking at prefixes.
                        if is_nursery(rule_group_id) {
                            return None;
                        }

                        if code.starts_with(&prefix) {
                            Some((rule_id.clone(), attrs.clone()))
                        } else {
                            None
                        }
                    },
                )
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
fn generate_rule_to_code(
    linter_to_rules: &BTreeMap<Ident, BTreeMap<String, LinterToRuleData>>,
) -> TokenStream {
    let mut rule_to_codes: HashMap<&Path, Vec<RuleToLinterData>> = HashMap::new();
    let mut linter_code_for_rule_match_arms = quote!();

    for (linter, map) in linter_to_rules {
        for (
            code,
            LinterToRuleData {
                rule_id,
                rule_group_id,
                attrs,
            },
        ) in map
        {
            rule_to_codes
                .entry(rule_id)
                .or_default()
                .push(RuleToLinterData {
                    linter,
                    code,
                    rule_group_id,
                    attrs,
                });
            let rule_name = rule_id.segments.last().unwrap();
            linter_code_for_rule_match_arms.extend(quote! {
                #(#attrs)* (Self::#linter, Rule::#rule_name) => Some(#code),
            });
        }
    }

    let mut rule_noqa_code_match_arms = quote!();
    let mut rule_group_match_arms = quote!();

    for (rule, codes) in rule_to_codes {
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

See also https://github.com/charliermarsh/ruff/issues/2186.
",
            rule.segments.last().unwrap().ident
        );
        let rule_name = rule.segments.last().unwrap();

        let RuleToLinterData {
            linter,
            code,
            rule_group_id,
            attrs,
        } = codes
            .iter()
            .sorted_by_key(|data| *data.linter == "Pylint")
            .next()
            .unwrap();

        rule_noqa_code_match_arms.extend(quote! {
            #(#attrs)* Rule::#rule_name => NoqaCode(crate::registry::Linter::#linter.common_prefix(), #code),
        });

        rule_group_match_arms.extend(quote! {
            #(#attrs)* Rule::#rule_name => #rule_group_id,
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

            pub fn is_nursery(&self) -> bool {
                matches!(self.group(), RuleGroup::Nursery)
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
    linter_to_rules: &BTreeMap<Ident, BTreeMap<String, LinterToRuleData>>,
    all_codes: &[TokenStream],
) -> TokenStream {
    let mut linter_into_iter_match_arms = quote!();
    for (linter, map) in linter_to_rules {
        let rule_paths = map.values().map(|LinterToRuleData { rule_id, attrs, .. }| {
            let rule_name = rule_id.segments.last().unwrap();
            quote!(#(#attrs)* Rule::#rule_name)
        });
        linter_into_iter_match_arms.extend(quote! {
            Linter::#linter => vec![#(#rule_paths,)*].into_iter(),
        });
    }

    quote! {
        impl IntoIterator for &Linter {
            type Item = Rule;
            type IntoIter = ::std::vec::IntoIter<Self::Item>;

            fn into_iter(self) -> Self::IntoIter {
                match self {
                    #linter_into_iter_match_arms
                }
            }
        }

        impl RuleCodePrefix {
            pub fn iter() -> ::std::vec::IntoIter<RuleCodePrefix> {
                vec![ #(#all_codes,)* ].into_iter()
            }
        }
    }
}

struct Entry {
    linter: Ident,
    code: LitStr,
    group: Path,
    rule: Path,
    attrs: Vec<Attribute>,
}

impl Parse for Entry {
    /// Parses a match arm such as `(Pycodestyle, "E112") => (RuleGroup::Nursery, Rule::NoIndentedBlock),`
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
        let rule: Path = pat_tuple.parse()?;
        let _: Token!(,) = input.parse()?;
        Ok(Entry {
            linter,
            code,
            group,
            rule,
            attrs,
        })
    }
}
