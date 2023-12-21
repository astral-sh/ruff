use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::string::ToString;

use anyhow::{bail, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use pep440_rs::{Version as Pep440Version, VersionSpecifiers};
use rustc_hash::FxHashMap;
use serde::{de, Deserialize, Deserializer, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::Applicability;
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;

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
    Default,
    Serialize,
    Deserialize,
    CacheKey,
    EnumIter,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PythonVersion {
    Py37,
    // Make sure to also change the default for `ruff_python_formatter::PythonVersion`
    // when changing the default here.
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
}

impl From<PythonVersion> for Pep440Version {
    fn from(version: PythonVersion) -> Self {
        let (major, minor) = version.as_tuple();
        Self::from_str(&format!("{major}.{minor}.100")).unwrap()
    }
}

impl PythonVersion {
    /// Return the latest supported Python version.
    pub const fn latest() -> Self {
        Self::Py312
    }

    pub const fn as_tuple(&self) -> (u8, u8) {
        match self {
            Self::Py37 => (3, 7),
            Self::Py38 => (3, 8),
            Self::Py39 => (3, 9),
            Self::Py310 => (3, 10),
            Self::Py311 => (3, 11),
            Self::Py312 => (3, 12),
        }
    }

    pub const fn major(&self) -> u8 {
        self.as_tuple().0
    }

    pub const fn minor(&self) -> u8 {
        self.as_tuple().1
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

    /// Return `true` if the current version supports [PEP 701].
    ///
    /// [PEP 701]: https://peps.python.org/pep-0701/
    pub fn supports_pep701(self) -> bool {
        self >= Self::Py312
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, CacheKey, is_macro::Is)]
pub enum PreviewMode {
    #[default]
    Disabled,
    Enabled,
}

impl From<bool> for PreviewMode {
    fn from(version: bool) -> Self {
        if version {
            PreviewMode::Enabled
        } else {
            PreviewMode::Disabled
        }
    }
}

/// Toggle for unsafe fixes.
/// `Hint` will not apply unsafe fixes but a message will be shown when they are available.
/// `Disabled` will not apply unsafe fixes or show a message.
/// `Enabled` will apply unsafe fixes.
#[derive(Debug, Copy, Clone, CacheKey, Default, PartialEq, Eq, is_macro::Is)]
pub enum UnsafeFixes {
    #[default]
    Hint,
    Disabled,
    Enabled,
}

impl From<bool> for UnsafeFixes {
    fn from(value: bool) -> Self {
        if value {
            UnsafeFixes::Enabled
        } else {
            UnsafeFixes::Disabled
        }
    }
}

impl UnsafeFixes {
    pub fn required_applicability(&self) -> Applicability {
        match self {
            Self::Enabled => Applicability::Unsafe,
            Self::Disabled | Self::Hint => Applicability::Safe,
        }
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
                    builder.add(Glob::new(&pattern)?);
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
    pub fn try_from_iter<I>(patterns: I) -> Result<Self, anyhow::Error>
    where
        I: IntoIterator<Item = FilePattern>,
    {
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
        // Rules in preview are included here even if preview mode is disabled; it's safe to ignore disabled rules
        let rules: RuleSet = prefixes.iter().flat_map(RuleSelector::all_rules).collect();
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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    CacheKey,
    EnumIter,
)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Language {
    #[default]
    Python,
    Pyi,
    Ipynb,
}

impl FromStr for Language {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "python" => Ok(Self::Python),
            "pyi" => Ok(Self::Pyi),
            "ipynb" => Ok(Self::Ipynb),
            _ => {
                bail!("Unrecognized language: `{s}`. Expected one of `python`, `pyi`, or `ipynb`.")
            }
        }
    }
}

impl From<Language> for PySourceType {
    fn from(value: Language) -> Self {
        match value {
            Language::Python => Self::Python,
            Language::Ipynb => Self::Ipynb,
            Language::Pyi => Self::Stub,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionPair {
    pub extension: String,
    pub language: Language,
}

impl ExtensionPair {
    const EXPECTED_PATTERN: &'static str = "<Extension>:<LanguageCode> pattern";
}

impl FromStr for ExtensionPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (extension_str, language_str) = {
            let tokens = s.split(':').collect::<Vec<_>>();
            if tokens.len() != 2 {
                bail!("Expected {}", Self::EXPECTED_PATTERN);
            }
            (tokens[0].trim(), tokens[1].trim())
        };
        let extension = extension_str.into();
        let language = Language::from_str(language_str)?;
        Ok(Self {
            extension,
            language,
        })
    }
}

impl From<ExtensionPair> for (String, Language) {
    fn from(value: ExtensionPair) -> Self {
        (value.extension, value.language)
    }
}
#[derive(Debug, Clone, Default, CacheKey)]
pub struct ExtensionMapping {
    mapping: FxHashMap<String, Language>,
}

impl ExtensionMapping {
    /// Return the [`Language`] for the given extension.
    pub fn get(&self, extension: &str) -> Option<Language> {
        self.mapping.get(extension).copied()
    }
}

impl From<FxHashMap<String, Language>> for ExtensionMapping {
    fn from(value: FxHashMap<String, Language>) -> Self {
        Self { mapping: value }
    }
}

impl FromIterator<ExtensionPair> for ExtensionMapping {
    fn from_iter<T: IntoIterator<Item = ExtensionPair>>(iter: T) -> Self {
        Self {
            mapping: iter
                .into_iter()
                .map(|pair| (pair.extension, pair.language))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum SerializationFormat {
    Text,
    Json,
    JsonLines,
    Junit,
    Grouped,
    Github,
    Gitlab,
    Pylint,
    Azure,
    Sarif,
}

impl Default for SerializationFormat {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(try_from = "String")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

/// Pattern to match an identifier.
///
/// # Notes
///
/// [`glob::Pattern`] matches a little differently than we ideally want to.
/// Specifically it uses `**` to match an arbitrary number of subdirectories,
/// luckily this not relevant since identifiers don't contains slashes.
///
/// For reference pep8-naming uses
/// [`fnmatch`](https://docs.python.org/3/library/fnmatch.html) for
/// pattern matching.
pub type IdentifierPattern = glob::Pattern;
