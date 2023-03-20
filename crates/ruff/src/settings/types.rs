use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::string::ToString;

use anyhow::{bail, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use pep440_rs::{Version as Pep440Version, VersionSpecifiers};
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;

use crate::fs;
use crate::registry::RuleSet;
use crate::rule_selector::RuleSelector;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    CacheKey,
    EnumIter,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum PythonVersion {
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
}

impl From<PythonVersion> for Pep440Version {
    fn from(version: PythonVersion) -> Self {
        let (major, minor) = version.as_tuple();
        Self::from_str(&format!("{major}.{minor}.100")).unwrap()
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

    pub fn get_minimum_supported_version(requires_version: &VersionSpecifiers) -> Option<Self> {
        let mut minimum_version = None;
        for python_version in PythonVersion::iter() {
            if requires_version
                .iter()
                .all(|specifier| specifier.contains(&python_version.into()))
            {
                minimum_version = Some(python_version);
                break;
            }
        }
        minimum_version
    }
}

#[derive(Debug, Clone, CacheKey, PartialEq, PartialOrd, Eq, Ord)]
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
        let absolute = fs::normalize_path(&pattern);
        Ok(Self::User(pattern, absolute))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilePatternSet {
    set: GlobSet,
    cache_key: u64,
}

impl FilePatternSet {
    pub fn try_from_vec(patterns: Vec<FilePattern>) -> Result<Self, anyhow::Error> {
        let mut builder = GlobSetBuilder::new();
        let mut hasher = CacheKeyHasher::new();

        for pattern in patterns {
            pattern.cache_key(&mut hasher);
            pattern.add_to(&mut builder)?;
        }

        let set = builder.build()?;

        Ok(FilePatternSet {
            set,
            cache_key: hasher.finish(),
        })
    }
}

impl Deref for FilePatternSet {
    type Target = GlobSet;

    fn deref(&self) -> &Self::Target {
        &self.set
    }
}

impl CacheKey for FilePatternSet {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.set.len());
        state.write_u64(self.cache_key);
    }
}

#[derive(Debug, Clone)]
pub struct PerFileIgnore {
    pub(crate) basename: String,
    pub(crate) absolute: PathBuf,
    pub(crate) rules: RuleSet,
}

impl PerFileIgnore {
    pub fn new(pattern: String, prefixes: &[RuleSelector], project_root: Option<&Path>) -> Self {
        let rules: RuleSet = prefixes.iter().flat_map(IntoIterator::into_iter).collect();
        let path = Path::new(&pattern);
        let absolute = match project_root {
            Some(project_root) => fs::normalize_path_to(path, project_root),
            None => fs::normalize_path(path),
        };

        Self {
            basename: pattern,
            absolute,
            rules,
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

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, JsonSchema, Hash)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum SerializationFormat {
    Text,
    Json,
    Junit,
    Grouped,
    Github,
    Gitlab,
    Pylint,
    Azure,
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
