#![allow(deprecated)]

use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::string::ToString;

use anyhow::{bail, Result};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use log::debug;
use pep440_rs::{Operator, Version as Pep440Version, Version, VersionSpecifier, VersionSpecifiers};
use rustc_hash::FxHashMap;
use serde::{de, Deserialize, Deserializer, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::Applicability;
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;

use crate::registry::RuleSet;
use crate::rule_selector::RuleSelector;
use crate::{display_settings, fs};

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
    Py313,
    // Remember to update the `latest()` function
    // when adding new versions here!
}

impl From<PythonVersion> for Pep440Version {
    fn from(version: PythonVersion) -> Self {
        let (major, minor) = version.as_tuple();
        Self::new([u64::from(major), u64::from(minor)])
    }
}

impl PythonVersion {
    /// Return the latest supported Python version.
    pub const fn latest() -> Self {
        Self::Py313
    }

    pub const fn as_tuple(&self) -> (u8, u8) {
        match self {
            Self::Py37 => (3, 7),
            Self::Py38 => (3, 8),
            Self::Py39 => (3, 9),
            Self::Py310 => (3, 10),
            Self::Py311 => (3, 11),
            Self::Py312 => (3, 12),
            Self::Py313 => (3, 13),
        }
    }

    pub const fn major(&self) -> u8 {
        self.as_tuple().0
    }

    pub const fn minor(&self) -> u8 {
        self.as_tuple().1
    }

    /// Infer the minimum supported [`PythonVersion`] from a `requires-python` specifier.
    pub fn get_minimum_supported_version(requires_version: &VersionSpecifiers) -> Option<Self> {
        /// Truncate a version to its major and minor components.
        fn major_minor(version: &Version) -> Option<Version> {
            let major = version.release().first()?;
            let minor = version.release().get(1)?;
            Some(Version::new([major, minor]))
        }

        // Extract the minimum supported version from the specifiers.
        let minimum_version = requires_version
            .iter()
            .filter(|specifier| {
                matches!(
                    specifier.operator(),
                    Operator::Equal
                        | Operator::EqualStar
                        | Operator::ExactEqual
                        | Operator::TildeEqual
                        | Operator::GreaterThan
                        | Operator::GreaterThanEqual
                )
            })
            .filter_map(|specifier| major_minor(specifier.version()))
            .min()?;

        debug!("Detected minimum supported `requires-python` version: {minimum_version}");

        // Find the Python version that matches the minimum supported version.
        PythonVersion::iter().find(|version| Version::from(*version) == minimum_version)
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

impl Display for PreviewMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Enabled => write!(f, "enabled"),
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

impl Display for UnsafeFixes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Hint => "hint",
                Self::Disabled => "disabled",
                Self::Enabled => "enabled",
            }
        )
    }
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

impl Display for FilePattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}",
            match self {
                Self::Builtin(pattern) => pattern,
                Self::User(pattern, _) => pattern.as_str(),
            }
        )
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
    // This field is only for displaying the internals
    // of `set`.
    #[allow(clippy::used_underscore_binding)]
    _set_internals: Vec<FilePattern>,
}

impl FilePatternSet {
    #[allow(clippy::used_underscore_binding)]
    pub fn try_from_iter<I>(patterns: I) -> Result<Self, anyhow::Error>
    where
        I: IntoIterator<Item = FilePattern>,
    {
        let mut builder = GlobSetBuilder::new();
        let mut hasher = CacheKeyHasher::new();

        let mut _set_internals = vec![];

        for pattern in patterns {
            _set_internals.push(pattern.clone());
            pattern.cache_key(&mut hasher);
            pattern.add_to(&mut builder)?;
        }

        let set = builder.build()?;

        Ok(FilePatternSet {
            set,
            cache_key: hasher.finish(),
            _set_internals,
        })
    }
}

impl Display for FilePatternSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self._set_internals.is_empty() {
            write!(f, "[]")?;
        } else {
            writeln!(f, "[")?;
            for pattern in &self._set_internals {
                writeln!(f, "\t{pattern},")?;
            }
            write!(f, "]")?;
        }
        Ok(())
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
    pub(crate) negated: bool,
    pub(crate) rules: RuleSet,
}

impl PerFileIgnore {
    pub fn new(
        mut pattern: String,
        prefixes: &[RuleSelector],
        project_root: Option<&Path>,
    ) -> Self {
        // Rules in preview are included here even if preview mode is disabled; it's safe to ignore disabled rules
        let rules: RuleSet = prefixes.iter().flat_map(RuleSelector::all_rules).collect();
        let negated = pattern.starts_with('!');
        if negated {
            pattern.drain(..1);
        }
        let path = Path::new(&pattern);
        let absolute = match project_root {
            Some(project_root) => fs::normalize_path_to(path, project_root),
            None => fs::normalize_path(path),
        };

        Self {
            basename: pattern,
            absolute,
            negated,
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
pub struct ExtensionMapping(FxHashMap<String, Language>);

impl ExtensionMapping {
    /// Return the [`Language`] for the given file.
    pub fn get(&self, path: &Path) -> Option<Language> {
        let ext = path.extension()?.to_str()?;
        self.0.get(ext).copied()
    }
}

impl From<FxHashMap<String, Language>> for ExtensionMapping {
    fn from(value: FxHashMap<String, Language>) -> Self {
        Self(value)
    }
}

impl FromIterator<ExtensionPair> for ExtensionMapping {
    fn from_iter<T: IntoIterator<Item = ExtensionPair>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .map(|pair| (pair.extension, pair.language))
                .collect(),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Hash, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum OutputFormat {
    // Remove the module level `#![allow(deprecated)` when removing the text variant.
    // Adding the `#[deprecated]` attribute to text creates clippy warnings about
    // using a deprecated item in the derived code and there seems to be no way to suppress the clippy error
    // other than disabling the warning for the entire module and/or moving `OutputFormat` to another module.
    #[deprecated(note = "Use `concise` or `full` instead")]
    Text,
    Concise,
    #[default]
    Full,
    Json,
    JsonLines,
    Junit,
    Grouped,
    Github,
    Gitlab,
    Pylint,
    Rdjson,
    Azure,
    Sarif,
}

impl Display for OutputFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Concise => write!(f, "concise"),
            Self::Full => write!(f, "full"),
            Self::Json => write!(f, "json"),
            Self::JsonLines => write!(f, "json_lines"),
            Self::Junit => write!(f, "junit"),
            Self::Grouped => write!(f, "grouped"),
            Self::Github => write!(f, "github"),
            Self::Gitlab => write!(f, "gitlab"),
            Self::Pylint => write!(f, "pylint"),
            Self::Rdjson => write!(f, "rdjson"),
            Self::Azure => write!(f, "azure"),
            Self::Sarif => write!(f, "sarif"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(try_from = "String")]
pub struct RequiredVersion(VersionSpecifiers);

impl TryFrom<String> for RequiredVersion {
    type Error = pep440_rs::VersionSpecifiersParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        // Treat `0.3.1` as `==0.3.1`, for backwards compatibility.
        if let Ok(version) = pep440_rs::Version::from_str(&value) {
            Ok(Self(VersionSpecifiers::from(
                VersionSpecifier::equals_version(version),
            )))
        } else {
            Ok(Self(VersionSpecifiers::from_str(&value)?))
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for RequiredVersion {
    fn schema_name() -> String {
        "RequiredVersion".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        gen.subschema_for::<String>()
    }
}

impl RequiredVersion {
    /// Return `true` if the given version is required.
    pub fn contains(&self, version: &pep440_rs::Version) -> bool {
        self.0.contains(version)
    }
}

impl Display for RequiredVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
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

#[derive(Debug, Clone, CacheKey)]
pub struct CompiledPerFileIgnore {
    pub absolute_matcher: GlobMatcher,
    pub basename_matcher: GlobMatcher,
    pub negated: bool,
    pub rules: RuleSet,
}

impl Display for CompiledPerFileIgnore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            fields = [
                self.absolute_matcher | globmatcher,
                self.basename_matcher | globmatcher,
                self.negated,
                self.rules,
            ]
        }
        Ok(())
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct CompiledPerFileIgnoreList {
    // Ordered as (absolute path matcher, basename matcher, rules)
    ignores: Vec<CompiledPerFileIgnore>,
}

impl CompiledPerFileIgnoreList {
    /// Given a list of patterns, create a `GlobSet`.
    pub fn resolve(per_file_ignores: Vec<PerFileIgnore>) -> Result<Self> {
        let ignores: Result<Vec<_>> = per_file_ignores
            .into_iter()
            .map(|per_file_ignore| {
                // Construct absolute path matcher.
                let absolute_matcher =
                    Glob::new(&per_file_ignore.absolute.to_string_lossy())?.compile_matcher();

                // Construct basename matcher.
                let basename_matcher = Glob::new(&per_file_ignore.basename)?.compile_matcher();

                Ok(CompiledPerFileIgnore {
                    absolute_matcher,
                    basename_matcher,
                    negated: per_file_ignore.negated,
                    rules: per_file_ignore.rules,
                })
            })
            .collect();
        Ok(Self { ignores: ignores? })
    }
}

impl Display for CompiledPerFileIgnoreList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.ignores.is_empty() {
            write!(f, "{{}}")?;
        } else {
            writeln!(f, "{{")?;
            for ignore in &self.ignores {
                writeln!(f, "\t{ignore}")?;
            }
            write!(f, "}}")?;
        }
        Ok(())
    }
}

impl Deref for CompiledPerFileIgnoreList {
    type Target = Vec<CompiledPerFileIgnore>;

    fn deref(&self) -> &Self::Target {
        &self.ignores
    }
}
