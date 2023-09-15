use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::codes::RuleCodePrefix;
use crate::codes::RuleIter;
use crate::registry::{Linter, Rule, RuleNamespace};
use crate::rule_redirects::get_redirect;
use crate::settings::types::PreviewMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuleSelector {
    /// Select all rules (includes rules in preview if enabled)
    All,
    /// Legacy category to select all rules in the "nursery" which predated preview mode
    #[deprecated(note = "The nursery was replaced with 'preview mode' which has no selector")]
    Nursery,
    /// Legacy category to select both the `mccabe` and `flake8-comprehensions` linters
    /// via a single selector.
    C,
    /// Legacy category to select both the `flake8-debugger` and `flake8-print` linters
    /// via a single selector.
    T,
    /// Select all rules for a given linter.
    Linter(Linter),
    /// Select all rules for a given linter with a given prefix.
    Prefix {
        prefix: RuleCodePrefix,
        redirected_from: Option<&'static str>,
    },
    /// Select an individual rule with a given prefix.
    Rule {
        prefix: RuleCodePrefix,
        redirected_from: Option<&'static str>,
    },
}

impl From<Linter> for RuleSelector {
    fn from(linter: Linter) -> Self {
        Self::Linter(linter)
    }
}

impl FromStr for RuleSelector {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ALL" => Ok(Self::All),
            #[allow(deprecated)]
            "NURSERY" => Ok(Self::Nursery),
            "C" => Ok(Self::C),
            "T" => Ok(Self::T),
            _ => {
                let (s, redirected_from) = match get_redirect(s) {
                    Some((from, target)) => (target, Some(from)),
                    None => (s, None),
                };

                let (linter, code) =
                    Linter::parse_code(s).ok_or_else(|| ParseError::Unknown(s.to_string()))?;

                if code.is_empty() {
                    return Ok(Self::Linter(linter));
                }

                // Does the selector select a single rule?
                let prefix = RuleCodePrefix::parse(&linter, code)
                    .map_err(|_| ParseError::Unknown(s.to_string()))?;

                if is_single_rule_selector(&prefix) {
                    Ok(Self::Rule {
                        prefix,
                        redirected_from,
                    })
                } else {
                    Ok(Self::Prefix {
                        prefix,
                        redirected_from,
                    })
                }
            }
        }
    }
}

/// Returns `true` if the [`RuleCodePrefix`] matches a single rule exactly
/// (e.g., `E225`, as opposed to `E2`).
pub(crate) fn is_single_rule_selector(prefix: &RuleCodePrefix) -> bool {
    let mut rules = prefix.rules();

    // The selector must match a single rule.
    let Some(rule) = rules.next() else {
        return false;
    };
    if rules.next().is_some() {
        return false;
    }

    // The rule must match the selector exactly.
    rule.noqa_code().suffix() == prefix.short_code()
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unknown rule selector: `{0}`")]
    // TODO(martin): tell the user how to discover rule codes via the CLI once such a command is
    // implemented (but that should of course be done only in ruff_cli and not here)
    Unknown(String),
}

impl RuleSelector {
    pub fn prefix_and_code(&self) -> (&'static str, &'static str) {
        match self {
            RuleSelector::All => ("", "ALL"),
            #[allow(deprecated)]
            RuleSelector::Nursery => ("", "NURSERY"),
            RuleSelector::C => ("", "C"),
            RuleSelector::T => ("", "T"),
            RuleSelector::Prefix { prefix, .. } | RuleSelector::Rule { prefix, .. } => {
                (prefix.linter().common_prefix(), prefix.short_code())
            }
            RuleSelector::Linter(l) => (l.common_prefix(), ""),
        }
    }
}

impl Serialize for RuleSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (prefix, code) = self.prefix_and_code();
        serializer.serialize_str(&format!("{prefix}{code}"))
    }
}

impl<'de> Deserialize<'de> for RuleSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // We are not simply doing:
        // let s: &str = Deserialize::deserialize(deserializer)?;
        // FromStr::from_str(s).map_err(de::Error::custom)
        // here because the toml crate apparently doesn't support that
        // (as of toml v0.6.0 running `cargo test` failed with the above two lines)
        deserializer.deserialize_str(SelectorVisitor)
    }
}

struct SelectorVisitor;

impl Visitor<'_> for SelectorVisitor {
    type Value = RuleSelector;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(
            "expected a string code identifying a linter or specific rule, or a partial rule code or ALL to refer to all rules",
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        FromStr::from_str(v).map_err(de::Error::custom)
    }
}

impl RuleSelector {
    /// Return all matching rules, regardless of whether they're in preview.
    pub fn all_rules(&self) -> impl Iterator<Item = Rule> + '_ {
        match self {
            RuleSelector::All => RuleSelectorIter::All(Rule::iter()),

            #[allow(deprecated)]
            RuleSelector::Nursery => {
                RuleSelectorIter::Nursery(Rule::iter().filter(Rule::is_nursery))
            }
            RuleSelector::C => RuleSelectorIter::Chain(
                Linter::Flake8Comprehensions
                    .rules()
                    .chain(Linter::McCabe.rules()),
            ),
            RuleSelector::T => RuleSelectorIter::Chain(
                Linter::Flake8Debugger
                    .rules()
                    .chain(Linter::Flake8Print.rules()),
            ),
            RuleSelector::Linter(linter) => RuleSelectorIter::Vec(linter.rules()),
            RuleSelector::Prefix { prefix, .. } | RuleSelector::Rule { prefix, .. } => {
                RuleSelectorIter::Vec(prefix.clone().rules())
            }
        }
    }

    /// Returns rules matching the selector, taking into account whether preview mode is enabled.
    pub fn rules(&self, preview: PreviewMode) -> impl Iterator<Item = Rule> + '_ {
        #[allow(deprecated)]
        self.all_rules().filter(move |rule| {
            // Always include rules that are not in preview or the nursery
            !(rule.is_preview() || rule.is_nursery())
            // Backwards compatibility allows selection of nursery rules by exact code or dedicated group
            || ((matches!(self, RuleSelector::Rule { .. }) || matches!(self, RuleSelector::Nursery { .. })) && rule.is_nursery())
            // Enabling preview includes all preview or nursery rules
            || preview.is_enabled()
        })
    }
}

pub enum RuleSelectorIter {
    All(RuleIter),
    Nursery(std::iter::Filter<RuleIter, fn(&Rule) -> bool>),
    Chain(std::iter::Chain<std::vec::IntoIter<Rule>, std::vec::IntoIter<Rule>>),
    Vec(std::vec::IntoIter<Rule>),
}

impl Iterator for RuleSelectorIter {
    type Item = Rule;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RuleSelectorIter::All(iter) => iter.next(),
            RuleSelectorIter::Nursery(iter) => iter.next(),
            RuleSelectorIter::Chain(iter) => iter.next(),
            RuleSelectorIter::Vec(iter) => iter.next(),
        }
    }
}

#[cfg(feature = "schemars")]
mod schema {
    use itertools::Itertools;
    use schemars::JsonSchema;
    use schemars::_serde_json::Value;
    use schemars::schema::{InstanceType, Schema, SchemaObject};
    use strum::IntoEnumIterator;

    use crate::registry::RuleNamespace;
    use crate::rule_selector::{Linter, RuleCodePrefix};
    use crate::RuleSelector;

    impl JsonSchema for RuleSelector {
        fn schema_name() -> String {
            "RuleSelector".to_string()
        }

        fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> Schema {
            Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::String.into()),
                enum_values: Some(
                    [
                        // Include the non-standard "ALL" and "NURSERY" selectors.
                        "ALL".to_string(),
                        "NURSERY".to_string(),
                        // Include the legacy "C" and "T" selectors.
                        "C".to_string(),
                        "T".to_string(),
                        // Include some common redirect targets for those legacy selectors.
                        "C9".to_string(),
                        "T1".to_string(),
                        "T2".to_string(),
                    ]
                    .into_iter()
                    .chain(
                        RuleCodePrefix::iter()
                            .map(|p| {
                                let prefix = p.linter().common_prefix();
                                let code = p.short_code();
                                format!("{prefix}{code}")
                            })
                            .chain(Linter::iter().filter_map(|l| {
                                let prefix = l.common_prefix();
                                (!prefix.is_empty()).then(|| prefix.to_string())
                            })),
                    )
                    // Filter out rule gated behind `#[cfg(feature = "unreachable-code")]`, which is
                    // off-by-default
                    .filter(|prefix| prefix != "RUF014")
                    .sorted()
                    .map(Value::String)
                    .collect(),
                ),
                ..SchemaObject::default()
            })
        }
    }
}

impl RuleSelector {
    pub fn specificity(&self) -> Specificity {
        match self {
            RuleSelector::All => Specificity::All,
            #[allow(deprecated)]
            RuleSelector::Nursery => Specificity::All,
            RuleSelector::T => Specificity::LinterGroup,
            RuleSelector::C => Specificity::LinterGroup,
            RuleSelector::Linter(..) => Specificity::Linter,
            RuleSelector::Rule { .. } => Specificity::Rule,
            RuleSelector::Prefix { prefix, .. } => {
                let prefix: &'static str = prefix.short_code();
                match prefix.len() {
                    1 => Specificity::Prefix1Char,
                    2 => Specificity::Prefix2Chars,
                    3 => Specificity::Prefix3Chars,
                    4 => Specificity::Prefix4Chars,
                    _ => panic!("RuleSelector::specificity doesn't yet support codes with so many characters"),
                }
            }
        }
    }
}

#[derive(EnumIter, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub enum Specificity {
    /// The specificity when selecting all rules (e.g., `--select ALL`).
    All,
    /// The specificity when selecting a legacy linter group (e.g., `--select C` or `--select T`).
    LinterGroup,
    /// The specificity when selecting a linter (e.g., `--select PLE` or `--select UP`).
    Linter,
    /// The specificity when selecting via a rule prefix with a one-character code (e.g., `--select PLE1`).
    Prefix1Char,
    /// The specificity when selecting via a rule prefix with a two-character code (e.g., `--select PLE12`).
    Prefix2Chars,
    /// The specificity when selecting via a rule prefix with a three-character code (e.g., `--select PLE123`).
    Prefix3Chars,
    /// The specificity when selecting via a rule prefix with a four-character code (e.g., `--select PLE1234`).
    Prefix4Chars,
    /// The specificity when selecting an individual rule (e.g., `--select PLE1205`).
    Rule,
}

#[cfg(feature = "clap")]
pub mod clap_completion {
    use clap::builder::{PossibleValue, TypedValueParser, ValueParserFactory};
    use strum::IntoEnumIterator;

    use crate::{
        codes::RuleCodePrefix,
        registry::{Linter, RuleNamespace},
        rule_selector::is_single_rule_selector,
        RuleSelector,
    };

    #[derive(Clone)]
    pub struct RuleSelectorParser;

    impl ValueParserFactory for RuleSelector {
        type Parser = RuleSelectorParser;

        fn value_parser() -> Self::Parser {
            RuleSelectorParser
        }
    }

    impl TypedValueParser for RuleSelectorParser {
        type Value = RuleSelector;

        fn parse_ref(
            &self,
            cmd: &clap::Command,
            arg: Option<&clap::Arg>,
            value: &std::ffi::OsStr,
        ) -> Result<Self::Value, clap::Error> {
            let value = value
                .to_str()
                .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8))?;

            value.parse().map_err(|_| {
                let mut error =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
                if let Some(arg) = arg {
                    error.insert(
                        clap::error::ContextKind::InvalidArg,
                        clap::error::ContextValue::String(arg.to_string()),
                    );
                }
                error.insert(
                    clap::error::ContextKind::InvalidValue,
                    clap::error::ContextValue::String(value.to_string()),
                );
                error
            })
        }

        fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
            Some(Box::new(
                std::iter::once(PossibleValue::new("ALL").help("all rules")).chain(
                    Linter::iter()
                        .filter_map(|l| {
                            let prefix = l.common_prefix();
                            (!prefix.is_empty()).then(|| PossibleValue::new(prefix).help(l.name()))
                        })
                        .chain(
                            RuleCodePrefix::iter()
                                // Filter out rule gated behind `#[cfg(feature = "unreachable-code")]`, which is
                                // off-by-default
                                .filter(|prefix| {
                                    format!(
                                        "{}{}",
                                        prefix.linter().common_prefix(),
                                        prefix.short_code()
                                    ) != "RUF014"
                                })
                                .filter_map(|prefix| {
                                    // Ex) `UP`
                                    if prefix.short_code().is_empty() {
                                        let code = prefix.linter().common_prefix();
                                        let name = prefix.linter().name();
                                        return Some(PossibleValue::new(code).help(name));
                                    }

                                    // Ex) `UP004`
                                    if is_single_rule_selector(&prefix) {
                                        let rule = prefix.rules().next()?;
                                        let code = format!(
                                            "{}{}",
                                            prefix.linter().common_prefix(),
                                            prefix.short_code()
                                        );
                                        let name: &'static str = rule.into();
                                        return Some(PossibleValue::new(code).help(name));
                                    }

                                    None
                                }),
                        ),
                ),
            ))
        }
    }
}
