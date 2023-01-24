use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::{Rule, RuleCodePrefix};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuleSelector(RuleCodePrefix);

impl FromStr for RuleSelector {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(RuleCodePrefix::from_str(s)?))
    }
}

impl From<RuleCodePrefix> for RuleSelector {
    fn from(prefix: RuleCodePrefix) -> Self {
        Self(prefix)
    }
}

impl IntoIterator for &RuleSelector {
    type IntoIter = ::std::vec::IntoIter<Self::Item>;
    type Item = Rule;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A const alternative to the `impl From<RuleCodePrefix> for RuleSelector`
// to let us keep the fields of RuleSelector private.
// Note that Rust doesn't yet support `impl const From<RuleCodePrefix> for
// RuleSelector` (see https://github.com/rust-lang/rust/issues/67792).
// TODO(martin): Remove once RuleSelector is an enum with Linter & Rule variants
pub(crate) const fn prefix_to_selector(prefix: RuleCodePrefix) -> RuleSelector {
    RuleSelector(prefix)
}

impl JsonSchema for RuleSelector {
    fn schema_name() -> String {
        "RuleSelector".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <RuleCodePrefix as JsonSchema>::json_schema(gen)
    }
}

impl RuleSelector {
    pub(crate) fn specificity(&self) -> Specificity {
        self.0.specificity()
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Specificity {
    All,
    Linter,
    Code1Char,
    Code2Chars,
    Code3Chars,
    Code4Chars,
    Code5Chars,
}
