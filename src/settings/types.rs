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
use crate::fs;
use crate::registry::{Rule, RuleSelector};

#[derive(
    Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PythonVersion {
    Py33,
    Py34,
    Py35,
    Py36,
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
            "py33" => Ok(PythonVersion::Py33),
            "py34" => Ok(PythonVersion::Py34),
            "py35" => Ok(PythonVersion::Py35),
            "py36" => Ok(PythonVersion::Py36),
            "py37" => Ok(PythonVersion::Py37),
            "py38" => Ok(PythonVersion::Py38),
            "py39" => Ok(PythonVersion::Py39),
            "py310" => Ok(PythonVersion::Py310),
            "py311" => Ok(PythonVersion::Py311),
            _ => Err(anyhow!("Unknown version: {string}")),
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
        let rules: FxHashSet<_> = prefixes.iter().flat_map(RuleSelector::codes).collect();
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
