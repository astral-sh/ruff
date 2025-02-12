use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::string::ToString;

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use log::debug;
use pep440_rs::{VersionSpecifier, VersionSpecifiers};
use rustc_hash::FxHashMap;
use serde::{de, Deserialize, Deserializer, Serialize};
use strum_macros::EnumIter;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::Applicability;
use ruff_macros::CacheKey;
use ruff_python_ast::{self as ast, PySourceType};

use crate::registry::RuleSet;
use crate::rule_selector::RuleSelector;
use crate::{display_settings, fs};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PythonVersion {
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl Default for PythonVersion {
    fn default() -> Self {
        // SAFETY: the unit test `default_python_version_works()` checks that this doesn't panic
        Self::try_from(ast::PythonVersion::default()).unwrap()
    }
}

impl TryFrom<ast::PythonVersion> for PythonVersion {
    type Error = String;

    fn try_from(value: ast::PythonVersion) -> Result<Self, Self::Error> {
        match value {
            ast::PythonVersion::PY37 => Ok(Self::Py37),
            ast::PythonVersion::PY38 => Ok(Self::Py38),
            ast::PythonVersion::PY39 => Ok(Self::Py39),
            ast::PythonVersion::PY310 => Ok(Self::Py310),
            ast::PythonVersion::PY311 => Ok(Self::Py311),
            ast::PythonVersion::PY312 => Ok(Self::Py312),
            ast::PythonVersion::PY313 => Ok(Self::Py313),
            _ => Err(format!("unrecognized python version {value}")),
        }
    }
}

impl From<PythonVersion> for ast::PythonVersion {
    fn from(value: PythonVersion) -> Self {
        let (major, minor) = value.as_tuple();
        Self { major, minor }
    }
}

impl From<PythonVersion> for pep440_rs::Version {
    fn from(version: PythonVersion) -> Self {
        let (major, minor) = version.as_tuple();
        Self::new([u64::from(major), u64::from(minor)])
    }
}

impl PythonVersion {
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

/// Represents a path to be passed to [`Glob::new`].
#[derive(Debug, Clone, CacheKey, PartialEq, PartialOrd, Eq, Ord)]
pub struct GlobPath {
    path: PathBuf,
}

impl GlobPath {
    /// Constructs a [`GlobPath`] by escaping any glob metacharacters in `root` and normalizing
    /// `path` to the escaped `root`.
    ///
    /// See [`fs::normalize_path_to`] for details of the normalization.
    pub fn normalize(path: impl AsRef<Path>, root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_string_lossy();
        let escaped = globset::escape(&root);
        let absolute = fs::normalize_path_to(path, escaped);
        Self { path: absolute }
    }

    pub fn into_inner(self) -> PathBuf {
        self.path
    }
}

impl Deref for GlobPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

#[derive(Debug, Clone, CacheKey, PartialEq, PartialOrd, Eq, Ord)]
pub enum FilePattern {
    Builtin(&'static str),
    User(String, GlobPath),
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
        Ok(Self::User(
            s.to_string(),
            GlobPath::normalize(s, fs::get_cwd()),
        ))
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

/// A glob pattern and associated data for matching file paths.
#[derive(Debug, Clone)]
pub struct PerFile<T> {
    /// The glob pattern used to construct the [`PerFile`].
    basename: String,
    /// The same pattern as `basename` but normalized to the project root directory.
    absolute: GlobPath,
    /// Whether the glob pattern should be negated (e.g. `!*.ipynb`)
    negated: bool,
    /// The per-file data associated with these glob patterns.
    data: T,
}

impl<T> PerFile<T> {
    /// Construct a new [`PerFile`] from the given glob `pattern` and containing `data`.
    ///
    /// If provided, `project_root` is used to construct a second glob pattern normalized to the
    /// project root directory. See [`fs::normalize_path_to`] for more details.
    fn new(mut pattern: String, project_root: Option<&Path>, data: T) -> Self {
        let negated = pattern.starts_with('!');
        if negated {
            pattern.drain(..1);
        }

        let project_root = project_root.unwrap_or(fs::get_cwd());

        Self {
            absolute: GlobPath::normalize(&pattern, project_root),
            basename: pattern,
            negated,
            data,
        }
    }
}

/// Per-file ignored linting rules.
///
/// See [`PerFile`] for details of the representation.
#[derive(Debug, Clone)]
pub struct PerFileIgnore(PerFile<RuleSet>);

impl PerFileIgnore {
    pub fn new(pattern: String, prefixes: &[RuleSelector], project_root: Option<&Path>) -> Self {
        // Rules in preview are included here even if preview mode is disabled; it's safe to ignore
        // disabled rules
        let rules: RuleSet = prefixes.iter().flat_map(RuleSelector::all_rules).collect();
        Self(PerFile::new(pattern, project_root, rules))
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

/// Like [`PerFile`] but with string globs compiled to [`GlobMatcher`]s for more efficient usage.
#[derive(Debug, Clone)]
pub struct CompiledPerFile<T> {
    pub absolute_matcher: GlobMatcher,
    pub basename_matcher: GlobMatcher,
    pub negated: bool,
    pub data: T,
}

impl<T> CompiledPerFile<T> {
    fn new(
        absolute_matcher: GlobMatcher,
        basename_matcher: GlobMatcher,
        negated: bool,
        data: T,
    ) -> Self {
        Self {
            absolute_matcher,
            basename_matcher,
            negated,
            data,
        }
    }
}

impl<T> CacheKey for CompiledPerFile<T>
where
    T: CacheKey,
{
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.absolute_matcher.cache_key(state);
        self.basename_matcher.cache_key(state);
        self.negated.cache_key(state);
        self.data.cache_key(state);
    }
}

impl<T> Display for CompiledPerFile<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            fields = [
                self.absolute_matcher | globmatcher,
                self.basename_matcher | globmatcher,
                self.negated,
                self.data,
            ]
        }
        Ok(())
    }
}

/// A sequence of [`CompiledPerFile<T>`].
#[derive(Debug, Clone, Default)]
pub struct CompiledPerFileList<T> {
    inner: Vec<CompiledPerFile<T>>,
}

impl<T> CacheKey for CompiledPerFileList<T>
where
    T: CacheKey,
{
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.inner.cache_key(state);
    }
}

impl<T> CompiledPerFileList<T> {
    /// Given a list of [`PerFile`] patterns, create a compiled set of globs.
    ///
    /// Returns an error if either of the glob patterns cannot be parsed.
    fn resolve(per_file_items: impl IntoIterator<Item = PerFile<T>>) -> Result<Self> {
        let inner: Result<Vec<_>> = per_file_items
            .into_iter()
            .map(|per_file_ignore| {
                // Construct absolute path matcher.
                let absolute_matcher = Glob::new(&per_file_ignore.absolute.to_string_lossy())
                    .with_context(|| format!("invalid glob {:?}", per_file_ignore.absolute))?
                    .compile_matcher();

                // Construct basename matcher.
                let basename_matcher = Glob::new(&per_file_ignore.basename)
                    .with_context(|| format!("invalid glob {:?}", per_file_ignore.basename))?
                    .compile_matcher();

                Ok(CompiledPerFile::new(
                    absolute_matcher,
                    basename_matcher,
                    per_file_ignore.negated,
                    per_file_ignore.data,
                ))
            })
            .collect();
        Ok(Self { inner: inner? })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T: std::fmt::Debug> CompiledPerFileList<T> {
    /// Return an iterator over the entries in `self` that match the input `path`.
    ///
    /// `debug_label` is used for [`debug!`] messages explaining why certain patterns were matched.
    pub(crate) fn iter_matches<'a, 'p>(
        &'a self,
        path: &'p Path,
        debug_label: &'static str,
    ) -> impl Iterator<Item = &'p T>
    where
        'a: 'p,
    {
        let file_name = path.file_name().expect("Unable to parse filename");
        self.inner.iter().filter_map(move |entry| {
            if entry.basename_matcher.is_match(file_name) {
                if entry.negated {
                    None
                } else {
                    debug!(
                        "{} for {:?} due to basename match on {:?}: {:?}",
                        debug_label,
                        path,
                        entry.basename_matcher.glob().regex(),
                        entry.data
                    );
                    Some(&entry.data)
                }
            } else if entry.absolute_matcher.is_match(path) {
                if entry.negated {
                    None
                } else {
                    debug!(
                        "{} for {:?} due to absolute match on {:?}: {:?}",
                        debug_label,
                        path,
                        entry.absolute_matcher.glob().regex(),
                        entry.data
                    );
                    Some(&entry.data)
                }
            } else if entry.negated {
                debug!(
                    "{} for {:?} due to negated pattern matching neither {:?} nor {:?}: {:?}",
                    debug_label,
                    path,
                    entry.basename_matcher.glob().regex(),
                    entry.absolute_matcher.glob().regex(),
                    entry.data
                );
                Some(&entry.data)
            } else {
                None
            }
        })
    }
}

impl<T> Display for CompiledPerFileList<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.inner.is_empty() {
            write!(f, "{{}}")?;
        } else {
            writeln!(f, "{{")?;
            for value in &self.inner {
                writeln!(f, "\t{value}")?;
            }
            write!(f, "}}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct CompiledPerFileIgnoreList(CompiledPerFileList<RuleSet>);

impl CompiledPerFileIgnoreList {
    /// Given a list of [`PerFileIgnore`] patterns, create a compiled set of globs.
    ///
    /// Returns an error if either of the glob patterns cannot be parsed.
    pub fn resolve(per_file_ignores: Vec<PerFileIgnore>) -> Result<Self> {
        Ok(Self(CompiledPerFileList::resolve(
            per_file_ignores.into_iter().map(|ignore| ignore.0),
        )?))
    }
}

impl Deref for CompiledPerFileIgnoreList {
    type Target = CompiledPerFileList<RuleSet>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for CompiledPerFileIgnoreList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Contains the target Python version for a given glob pattern.
///
/// See [`PerFile`] for details of the representation.
#[derive(Debug, Clone)]
pub struct PerFileTargetVersion(PerFile<ast::PythonVersion>);

impl PerFileTargetVersion {
    pub fn new(pattern: String, version: ast::PythonVersion, project_root: Option<&Path>) -> Self {
        Self(PerFile::new(pattern, project_root, version))
    }
}

#[derive(CacheKey, Clone, Debug, Default)]
pub struct CompiledPerFileTargetVersionList(CompiledPerFileList<ast::PythonVersion>);

impl CompiledPerFileTargetVersionList {
    /// Given a list of [`PerFileTargetVersion`] patterns, create a compiled set of globs.
    ///
    /// Returns an error if either of the glob patterns cannot be parsed.
    pub fn resolve(per_file_versions: Vec<PerFileTargetVersion>) -> Result<Self> {
        Ok(Self(CompiledPerFileList::resolve(
            per_file_versions.into_iter().map(|version| version.0),
        )?))
    }

    pub fn is_match(&self, path: &Path) -> Option<ast::PythonVersion> {
        self.0
            .iter_matches(path, "Setting Python version")
            .next()
            .copied()
    }
}

impl Display for CompiledPerFileTargetVersionList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_python_version_works() {
        super::PythonVersion::default();
    }
}
