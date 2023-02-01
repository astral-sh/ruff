use std::str::FromStr;

use schemars::_serde_json::Value;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::registry::{Rule, RuleCodePrefix, RuleIter};
use crate::rule_redirects::get_redirect;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuleSelector {
    /// All rules
    All,
    Prefix {
        prefix: RuleCodePrefix,
        redirected_from: Option<&'static str>,
    },
}

impl FromStr for RuleSelector {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "ALL" {
            Ok(Self::All)
        } else {
            let (s, redirected_from) = match get_redirect(s) {
                Some((from, target)) => (target, Some(from)),
                None => (s, None),
            };
            Ok(Self::Prefix {
                prefix: RuleCodePrefix::from_str(s)
                    .map_err(|_| ParseError::Unknown(s.to_string()))?,
                redirected_from,
            })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unknown rule selector `{0}`")]
    // TODO(martin): tell the user how to discover rule codes via the CLI once such a command is
    // implemented (but that should of course be done only in ruff_cli and not here)
    Unknown(String),
}

impl RuleSelector {
    pub fn short_code(&self) -> &'static str {
        match self {
            RuleSelector::All => "ALL",
            RuleSelector::Prefix { prefix, .. } => prefix.into(),
        }
    }
}

impl Serialize for RuleSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.short_code())
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
            "expected a string code identifying a linter or specific rule, or a partial rule code \
             or ALL to refer to all rules",
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        FromStr::from_str(v).map_err(de::Error::custom)
    }
}

impl From<RuleCodePrefix> for RuleSelector {
    fn from(prefix: RuleCodePrefix) -> Self {
        Self::Prefix {
            prefix,
            redirected_from: None,
        }
    }
}

impl IntoIterator for &RuleSelector {
    type IntoIter = RuleSelectorIter;
    type Item = Rule;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            RuleSelector::All => RuleSelectorIter::All(Rule::iter()),
            RuleSelector::Prefix { prefix, .. } => RuleSelectorIter::Prefix(prefix.into_iter()),
        }
    }
}

pub enum RuleSelectorIter {
    All(RuleIter),
    Prefix(std::vec::IntoIter<Rule>),
}

impl Iterator for RuleSelectorIter {
    type Item = Rule;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RuleSelectorIter::All(iter) => iter.next(),
            RuleSelectorIter::Prefix(iter) => iter.next(),
        }
    }
}

/// A const alternative to the `impl From<RuleCodePrefix> for RuleSelector`
// to let us keep the fields of RuleSelector private.
// Note that Rust doesn't yet support `impl const From<RuleCodePrefix> for
// RuleSelector` (see https://github.com/rust-lang/rust/issues/67792).
// TODO(martin): Remove once RuleSelector is an enum with Linter & Rule variants
pub(crate) const fn prefix_to_selector(prefix: RuleCodePrefix) -> RuleSelector {
    RuleSelector::Prefix {
        prefix,
        redirected_from: None,
    }
}

impl JsonSchema for RuleSelector {
    fn schema_name() -> String {
        "RuleSelector".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(
                std::iter::once("ALL".to_string())
                    .chain(RuleCodePrefix::iter().map(|s| s.as_ref().to_string()))
                    .map(Value::String)
                    .collect(),
            ),
            ..SchemaObject::default()
        })
    }
}

impl RuleSelector {
    pub(crate) fn specificity(&self) -> Specificity {
        match self {
            RuleSelector::All => Specificity::All,
            RuleSelector::Prefix { prefix, .. } => prefix.specificity(),
        }
    }
}

#[derive(EnumIter, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Specificity {
    All,
    Linter,
    Code1Char,
    Code2Chars,
    Code3Chars,
    Code4Chars,
    Code5Chars,
}
