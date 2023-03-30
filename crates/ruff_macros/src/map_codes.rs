use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parenthesized, parse::Parse, spanned::Spanned, Attribute, Error, Expr, ExprCall, ExprMatch,
    Ident, ItemFn, LitStr, Pat, Path, Stmt, Token,
};

use crate::rule_code_prefix::{get_prefix_ident, if_all_same};

pub fn map_codes(func: &ItemFn) -> syn::Result<TokenStream> {
    let Some(last_stmt) = func.block.stmts.last() else {
        return Err(Error::new(func.block.span(), "expected body to end in an expression"));
    };
    let Stmt::Expr(Expr::Call(ExprCall{args: some_args, ..})) = last_stmt else {
        return Err(Error::new(last_stmt.span(), "expected last expression to be Some(match (..) { .. })"))
    };
    let mut some_args = some_args.into_iter();
    let (Some(Expr::Match(ExprMatch { arms, .. })), None) = (some_args.next(), some_args.next()) else {
        return Err(Error::new(last_stmt.span(), "expected last expression to be Some(match (..) { .. })"))
    };

    let mut linters: BTreeMap<Ident, BTreeMap<String, (Path, Vec<Attribute>)>> = BTreeMap::new();

    for arm in arms {
        if matches!(arm.pat, Pat::Wild(..)) {
            break;
        }

        let entry = syn::parse::<Entry>(arm.into_token_stream().into())?;
        linters
            .entry(entry.linter)
            .or_default()
            .insert(entry.code.value(), (entry.rule, entry.attrs));
    }

    let linter_idents: Vec<_> = linters.keys().collect();

    let mut out = quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum RuleCodePrefix {
            #(#linter_idents(#linter_idents),)*
        }

        impl RuleCodePrefix {
            pub fn linter(&self) -> &'static Linter {
                match self {
                    #(Self::#linter_idents(..) => &crate::registry::Linter::#linter_idents,)*
                }
            }

            pub fn short_code(&self) -> &'static str {
                match self {
                    #(Self::#linter_idents(code) => code.into(),)*
                }
            }
        }
    };

    for (linter, map) in &linters {
        out.extend(super::rule_code_prefix::expand(
            linter,
            map.iter().map(|(k, v)| (k.as_str(), &v.1)),
        ));

        out.extend(quote! {
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

    for (linter, map) in &linters {
        let mut full_map: HashMap<_, _> = map
            .iter()
            .map(|(code, rule)| (code.clone(), vec![rule.clone()]))
            .collect();
        for code in map.keys() {
            for i in 1..=code.len() {
                let prefix = code[..i].to_string();
                let rules: Vec<_> = map
                    .iter()
                    .filter_map(|(code, rules)| {
                        if code.starts_with(&prefix) {
                            Some(rules)
                        } else {
                            None
                        }
                    })
                    .cloned()
                    .collect();
                full_map.insert(prefix, rules);
            }
        }

        for (code, names) in &full_map {
            let prefix_ident = get_prefix_ident(code);
            let attr = match if_all_same(names.iter().map(|(_, attrs)| attrs)) {
                Some(attr) => quote!(#(#attr)*),
                None => quote!(),
            };
            all_codes.push(quote! {
                #attr Self::#linter(#linter::#prefix_ident)
            });
        }

        let mut prefix_into_iter_match_arms = quote!();

        for (code, rules) in full_map {
            let rule_paths = rules.iter().map(|(path, attrs)| quote!(#(#attrs)* #path));
            let prefix_ident = get_prefix_ident(&code);
            let attr = match if_all_same(rules.iter().map(|(_, attrs)| attrs)) {
                Some(attr) => quote!(#(#attr)*),
                None => quote!(),
            };
            prefix_into_iter_match_arms.extend(quote! {
                #attr #linter::#prefix_ident => vec![#(#rule_paths,)*].into_iter(),
            });
        }

        out.extend(quote! {
            impl IntoIterator for &#linter {
                type Item = Rule;
                type IntoIter = ::std::vec::IntoIter<Self::Item>;

                fn into_iter(self) -> Self::IntoIter {
                    match self { #prefix_into_iter_match_arms }
                }
            }
        });
    }

    out.extend(quote! {
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

    out.extend(quote! {
        impl RuleCodePrefix {
            pub fn parse(linter: &Linter, code: &str) -> Result<Self, crate::registry::FromCodeError> {
                use std::str::FromStr;

                Ok(match linter {
                    #(Linter::#linter_idents => RuleCodePrefix::#linter_idents(#linter_idents::from_str(code).map_err(|_| crate::registry::FromCodeError::Unknown)?),)*
                })
            }
        }
    });

    #[allow(clippy::type_complexity)]
    let mut rule_to_codes: HashMap<&Path, Vec<(&Ident, &str, &Vec<Attribute>)>> = HashMap::new();
    let mut linter_code_for_rule_match_arms = quote!();

    for (linter, map) in &linters {
        for (code, (rule, attrs)) in map {
            rule_to_codes
                .entry(rule)
                .or_default()
                .push((linter, code, attrs));
            linter_code_for_rule_match_arms.extend(quote! {
                #(#attrs)* (Self::#linter, #rule) => Some(#code),
            });
        }
    }

    let mut rule_noqa_code_match_arms = quote!();

    for (rule, codes) in rule_to_codes {
        assert!(
            codes.len() == 1,
            "
            The mapping of multiple codes to one rule has been disabled due to UX concerns (it would
            be confusing if violations were reported under a different code than the code you selected).

            We firstly want to allow rules to be selected by their names (and report them by name),
            and before we can do that we have to rename all our rules to match our naming convention
            (see CONTRIBUTING.md) because after that change every rule rename will be a breaking change.

            See also https://github.com/charliermarsh/ruff/issues/2186.

            (this was triggered by {} being mapped to multiple codes)
            ",
            rule.segments.last().unwrap().ident
        );

        let (linter, code, attrs) = codes
            .iter()
            .sorted_by_key(|(l, ..)| *l == "Pylint") // TODO: more sophisticated sorting
            .next()
            .unwrap();

        rule_noqa_code_match_arms.extend(quote! {
            #(#attrs)* #rule => NoqaCode(crate::registry::Linter::#linter.common_prefix(), #code),
        });
    }

    out.extend(quote! {
        impl crate::registry::Rule {
            pub fn noqa_code(&self) -> NoqaCode {
                use crate::registry::RuleNamespace;

                match self {
                    #rule_noqa_code_match_arms
                    // TODO: support rules without codes
                    // rule => rule.as_ref()
                }
            }
        }

        impl crate::registry::Linter {
            pub fn code_for_rule(&self, rule: Rule) -> Option<&'static str> {
                match (self, rule) {
                    #linter_code_for_rule_match_arms
                    _ => None,
                }
            }
        }
    });

    let mut linter_into_iter_match_arms = quote!();
    for (linter, map) in &linters {
        let rule_paths = map.values().map(|(path, attrs)| quote!(#(#attrs)* #path));
        linter_into_iter_match_arms.extend(quote! {
            crate::registry::Linter::#linter => vec![#(#rule_paths,)*].into_iter(),
        });
    }

    out.extend(quote! {

        impl IntoIterator for &crate::registry::Linter {
            type Item = Rule;
            type IntoIter = ::std::vec::IntoIter<Self::Item>;

            fn into_iter(self) -> Self::IntoIter {
                match self {
                    #linter_into_iter_match_arms
                }
            }
        }

    });

    out.extend(quote! {
        impl RuleCodePrefix {
            pub fn iter() -> ::std::vec::IntoIter<RuleCodePrefix> {
                vec![ #(#all_codes,)* ].into_iter()
            }
        }
    });

    Ok(out)
}

struct Entry {
    linter: Ident,
    code: LitStr,
    rule: Path,
    attrs: Vec<Attribute>,
}

impl Parse for Entry {
    /// Parses a match arm such as `(Pycodestyle, "E101") => Rule::MixedSpacesAndTabs,`
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = Attribute::parse_outer(input)?;
        let pat_tuple;
        parenthesized!(pat_tuple in input);
        let linter: Ident = pat_tuple.parse()?;
        let _: Token!(,) = pat_tuple.parse()?;
        let code: LitStr = pat_tuple.parse()?;
        let _: Token!(=>) = input.parse()?;
        let rule: Path = input.parse()?;
        let _: Token!(,) = input.parse()?;
        Ok(Entry {
            linter,
            code,
            rule,
            attrs,
        })
    }
}
