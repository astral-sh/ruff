use std::hash::Hash;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, bail, Result};
use clap::ValueEnum;
use globset::{Glob, GlobSetBuilder};
use rustc_hash::FxHashSet;
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};

use super::hashable::HashableHashSet;
use crate::registry::Rule;
use crate::rule_selector::RuleSelector;
use crate::{fs, warn_user_once};

#[derive(
    Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PythonVersion {
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
}

impl FromStr for PythonVersion {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "py33" | "py34" | "py35" | "py36" => {
                warn_user_once!(
                    "Specified a version below the minimum supported Python version. Defaulting \
                     to Python 3.7."
                );
                Ok(Self::Py37)
            }
            "py37" => Ok(Self::Py37),
            "py38" => Ok(Self::Py38),
            "py39" => Ok(Self::Py39),
            "py310" => Ok(Self::Py310),
            "py311" => Ok(Self::Py311),
            _ => Err(anyhow!("Unknown version: {string} (try: \"py37\")")),
        }
    }
}

impl PythonVersion {
    pub const fn as_tuple(&self) -> (u32, u32) {
        match self {
            Self::Py37 => (3, 7),
            Self::Py38 => (3, 8),
            Self::Py39 => (3, 9),
            Self::Py310 => (3, 10),
            Self::Py311 => (3, 11),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub enum FilePattern {
    Builtin(&'static str),
    User(String, PathBuf),
}

impl FilePattern {
    pub fn add_to(self, builder: &mut GlobSetBuilder) -> Result<()> {
        match self {
            FilePattern::Builtin(pattern) => {
                builder.add(Glob::from_str(pattern)?);
            }
            FilePattern::User(pattern, absolute) => {
                // Add the absolute path.
                builder.add(Glob::new(&absolute.to_string_lossy())?);

                // Add basename path.
                if !pattern.contains(std::path::MAIN_SEPARATOR) {
                    builder.add(Glob::from_str(&pattern)?);
                }
            }
        }
        Ok(())
    }
}

impl FromStr for FilePattern {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pattern = s.to_string();
        let absolute = fs::normalize_path(Path::new(&pattern));
        Ok(Self::User(pattern, absolute))
    }
}

#[derive(Debug, Clone)]
pub struct PerFileIgnore {
    pub(crate) basename: String,
    pub(crate) absolute: PathBuf,
    pub(crate) rules: HashableHashSet<Rule>,
}

impl PerFileIgnore {
    pub fn new(pattern: String, prefixes: &[RuleSelector], project_root: Option<&Path>) -> Self {
        let rules: FxHashSet<_> = prefixes.iter().flat_map(IntoIterator::into_iter).collect();
        let path = Path::new(&pattern);
        let absolute = match project_root {
            Some(project_root) => fs::normalize_path_to(path, project_root),
            None => fs::normalize_path(path),
        };

        Self {
            basename: pattern,
            absolute,
            rules: rules.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternPrefixPair {
    pub pattern: String,
    pub prefix: RuleSelector,
}

impl PatternPrefixPair {
    const EXPECTED_PATTERN: &'static str = "<FilePattern>:<RuleCode> pattern";
}

impl<'de> Deserialize<'de> for PatternPrefixPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str_result = String::deserialize(deserializer)?;
        Self::from_str(str_result.as_str()).map_err(|_| {
            de::Error::invalid_value(
                de::Unexpected::Str(str_result.as_str()),
                &Self::EXPECTED_PATTERN,
            )
        })
    }
}

impl FromStr for PatternPrefixPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (pattern_str, code_string) = {
            let tokens = s.split(':').collect::<Vec<_>>();
            if tokens.len() != 2 {
                bail!("Expected {}", Self::EXPECTED_PATTERN);
            }
            (tokens[0].trim(), tokens[1].trim())
        };
        let pattern = pattern_str.into();
        let prefix = RuleSelector::from_str(code_string)?;
        Ok(Self { pattern, prefix })
    }
}

#[derive(
    Clone, Copy, ValueEnum, PartialEq, Eq, Serialize, Deserialize, Debug, JsonSchema, Hash,
)]
#[serde(rename_all = "kebab-case")]
pub enum SerializationFormat {
    Text,
    Json,
    Junit,
    Grouped,
    Github,
    Gitlab,
    Pylint,
}

impl Default for SerializationFormat {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Hash)]
#[serde(try_from = "String")]
pub struct Version(String);

impl TryFrom<String> for Version {
    type Error = semver::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        semver::Version::parse(&value).map(|_| Self(value))
    }
}

impl Deref for Version {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
