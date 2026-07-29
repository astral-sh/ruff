use std::hash::Hash;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use ruff_ranged_value::{RangedValue, ValueSource};

use crate::codes::RuleIter;
use crate::codes::{RuleCodePrefix, RuleGroup};
use crate::preview::is_human_readable_names_enabled;
use crate::registry::{Linter, Rule, RuleNamespace};
use crate::rule_redirects::get_redirect;
use crate::settings::types::PreviewMode;
use crate::warn_user_once_by_message;

/// A potential rule selector that has not yet been validated and tracks its source.
///
/// If you add a new field that uses this type, be sure to update `rule-codes-in-selectors`
/// (`RUF201`) to validate the additional selector field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct UnresolvedRuleSelector(RangedValue<String>);

impl UnresolvedRuleSelector {
    pub fn resolve(&self, preview: PreviewMode) -> Result<RuleSelector, RuleResolutionError> {
        RuleSelector::from_str(self.0.as_str()).or_else(|_| {
            let kind = if let Ok(rule) = Rule::from_name(self.0.as_str()) {
                if is_human_readable_names_enabled(preview) {
                    return Ok(RuleSelector::rule(rule));
                }
                RuleResolutionErrorKind::PreviewName
            } else if matches!(self.0.as_str(), "PREVIEW" | "NURSERY") {
                RuleResolutionErrorKind::Removed
            } else {
                RuleResolutionErrorKind::Unknown
            };
            Err(RuleResolutionError::from_selector(self, kind))
        })
    }

    pub fn new(selector: impl Into<String>, source: ValueSource) -> Self {
        Self(RangedValue::new(selector.into(), source))
    }

    pub fn cli(selector: impl Into<String>) -> Self {
        Self::new(selector, ValueSource::Cli)
    }

    pub fn source(&self) -> &ValueSource {
        self.0.source()
    }
}

#[derive(Debug)]
enum RuleResolutionErrorKind {
    Removed,
    Unknown,
    PreviewName,
}

#[derive(Debug)]
pub struct RuleResolutionError {
    selector: String,
    setting: Option<&'static str>,
    source: ValueSource,
    kind: RuleResolutionErrorKind,
}

impl RuleResolutionError {
    fn from_selector(unresolved: &UnresolvedRuleSelector, kind: RuleResolutionErrorKind) -> Self {
        Self {
            selector: unresolved.0.to_string(),
            setting: None,
            source: unresolved.0.source().clone(),
            kind,
        }
    }

    /// Attach the configuration option where the error occurred.
    #[must_use]
    pub fn with_setting(mut self, setting: &'static str) -> Self {
        self.setting = Some(setting);
        self
    }

    pub fn log_warning(&self) {
        warn_user_once_by_message!("{}", self);
    }
}

impl std::fmt::Display for RuleResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            selector,
            setting,
            source,
            kind,
        } = self;
        let setting = match setting {
            Some(setting) => format_args!(" in `{}`", *setting),
            None => format_args!(""),
        };
        let source = match &source {
            ValueSource::File(path) => format_args!("`{}`", path.as_path()),
            ValueSource::Cli => format_args!("the CLI"),
            ValueSource::Editor => format_args!("the editor configuration"),
            ValueSource::UvWorkspace => format_args!("uv workspace metadata"),
        };
        match kind {
            RuleResolutionErrorKind::Removed => {
                write!(f, "Removed selector `{selector}`{setting} from {source}")
            }
            RuleResolutionErrorKind::Unknown => write!(
                f,
                "Unknown rule selector `{selector}`{setting} from {source}"
            ),
            RuleResolutionErrorKind::PreviewName => write!(
                f,
                "Invalid selector `{selector}`{setting} from {source}. \
                    Selecting rules by name requires preview mode"
            ),
        }
    }
}

impl std::error::Error for RuleResolutionError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuleSelector {
    /// Select all rules (includes rules in preview if enabled)
    All,
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
    /// Select an individual rule.
    Rule {
        rule: Rule,
        redirected_from: Option<&'static str>,
    },
}

impl RuleSelector {
    pub(crate) const fn rule(rule: Rule) -> Self {
        Self::Rule {
            rule,
            redirected_from: None,
        }
    }
}

impl Ord for RuleSelector {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // TODO(zanieb): We ought to put "ALL" and "Linter" selectors
        // above those that are rule specific but it's not critical for now
        self.prefix_and_code().cmp(&other.prefix_and_code())
    }
}

impl PartialOrd for RuleSelector {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for RuleSelector {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // **Changes should be reflected in `parse_no_redirect` as well**
        match s {
            "ALL" => Ok(Self::All),
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

                let prefix = RuleCodePrefix::parse(&linter, code)
                    .map_err(|_| ParseError::Unknown(s.to_string()))?;

                if let Some(rule) = prefix.as_rule() {
                    Ok(Self::Rule {
                        rule,
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

impl RuleCodePrefix {
    /// Returns the rule if this prefix matches a single rule exactly
    /// (e.g., `E225`, as opposed to `E2`).
    pub(crate) fn as_rule(&self) -> Option<Rule> {
        let mut rules = self.rules();

        // The selector must match a single rule.
        let rule = rules.next()?;
        if rules.next().is_some() {
            return None;
        }

        // The rule must match the selector exactly.
        (rule.noqa_code().suffix() == self.short_code()).then_some(rule)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unknown rule selector: `{0}`")]
    // TODO(martin): tell the user how to discover rule codes via the CLI once such a command is
    // implemented (but that should of course be done only in ruff and not here)
    Unknown(String),
}

impl RuleSelector {
    pub fn prefix_and_code(&self) -> (&'static str, &'static str) {
        match self {
            RuleSelector::All => ("", "ALL"),
            RuleSelector::C => ("", "C"),
            RuleSelector::T => ("", "T"),
            RuleSelector::Prefix { prefix, .. } => {
                (prefix.linter().common_prefix(), prefix.short_code())
            }
            RuleSelector::Rule { rule, .. } => rule.noqa_code().into_parts(),
            RuleSelector::Linter(l) => (l.common_prefix(), ""),
        }
    }
}

impl RuleSelector {
    /// Return all matching rules, regardless of rule group filters like preview and deprecated.
    pub fn all_rules(&self) -> impl Iterator<Item = Rule> + use<> {
        match self {
            RuleSelector::All => RuleSelectorIter::All(Rule::iter()),

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
            RuleSelector::Prefix { prefix, .. } => RuleSelectorIter::Vec(prefix.clone().rules()),
            RuleSelector::Rule { rule, .. } => RuleSelectorIter::Once(std::iter::once(*rule)),
        }
    }

    /// Returns rules matching the selector, taking into account rule groups like preview and deprecated.
    pub fn rules<'a>(&'a self, preview: &PreviewOptions) -> impl Iterator<Item = Rule> + use<'a> {
        let preview_enabled = preview.mode.is_enabled();
        let preview_require_explicit = preview.require_explicit;

        self.all_rules().filter(move |rule| {
            match rule.group() {
                // Always include stable rules
                RuleGroup::Stable { .. } => true,
                // Enabling preview includes all preview rules unless explicit selection is turned on
                RuleGroup::Preview { .. } => {
                    preview_enabled && (self.is_exact() || !preview_require_explicit)
                }
                // Deprecated rules are excluded by default unless explicitly selected
                RuleGroup::Deprecated { .. } => !preview_enabled && self.is_exact(),
                // Removed rules are included if explicitly selected but will error downstream
                RuleGroup::Removed { .. } => self.is_exact(),
            }
        })
    }

    /// Returns true if this selector is exact i.e. selects a single rule by code
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Rule { .. })
    }
}

pub enum RuleSelectorIter {
    All(RuleIter),
    Chain(std::iter::Chain<std::vec::IntoIter<Rule>, std::vec::IntoIter<Rule>>),
    Vec(std::vec::IntoIter<Rule>),
    Once(std::iter::Once<Rule>),
}

impl Iterator for RuleSelectorIter {
    type Item = Rule;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RuleSelectorIter::All(iter) => iter.next(),
            RuleSelectorIter::Chain(iter) => iter.next(),
            RuleSelectorIter::Vec(iter) => iter.next(),
            RuleSelectorIter::Once(iter) => iter.next(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewOptions {
    pub mode: PreviewMode,
    /// If true, preview rule selection requires explicit codes e.g. not prefixes.
    /// Generally this should be derived from the user-facing `explicit-preview-rules` option.
    pub require_explicit: bool,
}

#[cfg(feature = "schemars")]
mod schema {
    use itertools::Itertools;
    use schemars::{JsonSchema, Schema, SchemaGenerator};
    use serde_json::Value;
    use strum::IntoEnumIterator;

    use crate::codes::Rule;
    use crate::registry::RuleNamespace;
    use crate::rule_selector::{Linter, RuleCodePrefix};
    use crate::{RuleSelector, UnresolvedRuleSelector};

    impl JsonSchema for UnresolvedRuleSelector {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            std::borrow::Cow::Borrowed("RuleSelector")
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            let enum_values: Vec<String> = [
                // Include the non-standard "ALL" selectors.
                "ALL".to_string(),
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
            .filter(|p| {
                // Exclude removed rules and prefixes where all of the rules are removed
                match RuleSelector::parse_no_redirect(p) {
                    Ok(RuleSelector::Rule { rule, .. }) => !rule.is_removed(),
                    Ok(RuleSelector::Prefix { prefix, .. }) => {
                        !prefix.rules().all(|rule| rule.is_removed())
                    }
                    _ => true,
                }
            })
            .filter(|_rule| {
                // Filter out all test-only rules
                #[cfg(any(feature = "test-rules", test))]
                #[expect(clippy::used_underscore_binding)]
                if _rule.starts_with("RUF9") || _rule == "PLW0101" {
                    return false;
                }

                true
            })
            .flat_map(|code| {
                Rule::from_code(&code)
                    .map(|rule| rule.name().to_string())
                    .into_iter()
                    .chain(std::iter::once(code))
            })
            .sorted()
            .collect();

            let mut schema = schemars::json_schema!({ "type": "string" });
            schema.ensure_object().insert(
                "enum".to_string(),
                Value::Array(enum_values.into_iter().map(Value::String).collect()),
            );

            schema
        }
    }
}

impl RuleSelector {
    pub fn specificity(&self) -> Specificity {
        match self {
            RuleSelector::All => Specificity::All,
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
                    _ => panic!(
                        "RuleSelector::specificity doesn't yet support codes with so many characters"
                    ),
                }
            }
        }
    }

    /// Parse [`RuleSelector`] from a string; but do not follow redirects.
    pub fn parse_no_redirect(s: &str) -> Result<Self, ParseError> {
        // **Changes should be reflected in `from_str` as well**
        match s {
            "ALL" => Ok(Self::All),
            "C" => Ok(Self::C),
            "T" => Ok(Self::T),
            _ => {
                let (linter, code) =
                    Linter::parse_code(s).ok_or_else(|| ParseError::Unknown(s.to_string()))?;

                if code.is_empty() {
                    return Ok(Self::Linter(linter));
                }

                let prefix = RuleCodePrefix::parse(&linter, code)
                    .map_err(|_| ParseError::Unknown(s.to_string()))?;

                if let Some(rule) = prefix.as_rule() {
                    Ok(Self::Rule {
                        rule,
                        redirected_from: None,
                    })
                } else {
                    Ok(Self::Prefix {
                        prefix,
                        redirected_from: None,
                    })
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
        codes::{Rule, RuleCodePrefix},
        registry::{Linter, RuleNamespace},
        rule_selector::UnresolvedRuleSelector,
    };

    #[derive(Clone)]
    pub struct UnresolvedRuleSelectorParser;

    impl ValueParserFactory for UnresolvedRuleSelector {
        type Parser = UnresolvedRuleSelectorParser;

        fn value_parser() -> Self::Parser {
            UnresolvedRuleSelectorParser
        }
    }

    impl TypedValueParser for UnresolvedRuleSelectorParser {
        type Value = UnresolvedRuleSelector;

        fn parse_ref(
            &self,
            _cmd: &clap::Command,
            _arg: Option<&clap::Arg>,
            value: &std::ffi::OsStr,
        ) -> Result<Self::Value, clap::Error> {
            let value = value
                .to_str()
                .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8))?;

            Ok(UnresolvedRuleSelector::cli(value))
        }

        fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
            Some(Box::new(
                std::iter::once(PossibleValue::new("ALL").help("all rules")).chain(
                    Linter::iter()
                        .filter_map(|l| {
                            let prefix = l.common_prefix();
                            (!prefix.is_empty()).then(|| PossibleValue::new(prefix).help(l.name()))
                        })
                        .chain(RuleCodePrefix::iter().filter_map(|prefix| {
                            // Ex) `UP`
                            if prefix.short_code().is_empty() {
                                let code = prefix.linter().common_prefix();
                                let name = prefix.linter().name();
                                return Some(PossibleValue::new(code).help(name));
                            }

                            // Ex) `UP004`
                            if let Some(rule) = prefix.as_rule() {
                                let code = format!(
                                    "{}{}",
                                    prefix.linter().common_prefix(),
                                    prefix.short_code()
                                );
                                return Some(PossibleValue::new(code).help(rule.name().as_str()));
                            }

                            None
                        }))
                        .chain(Rule::iter().map(|rule| {
                            PossibleValue::new(rule.name().as_str())
                                .help(rule.noqa_code().to_string())
                        })),
                ),
            ))
        }
    }
}
