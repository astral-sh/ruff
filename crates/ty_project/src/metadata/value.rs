use crate::Db;
use crate::glob::{
    AbsolutePortableGlobPattern, PortableGlobError, PortableGlobKind, PortableGlobPattern,
};
use ruff_db::ranged_value::{RangedValue, ValueSource};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_macros::Combine;
use ruff_text_size::TextRange;
use std::fmt;
use std::fmt::Formatter;

/// A possibly relative path in a configuration file.
///
/// Relative paths in configuration files or from CLI options
/// require different anchoring:
///
/// * CLI: The path is relative to the current working directory
/// * Configuration file: The path is relative to the project's root.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Combine,
)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RelativePathBuf(RangedValue<SystemPathBuf>);

impl RelativePathBuf {
    pub fn new(path: impl AsRef<SystemPath>, source: ValueSource) -> Self {
        Self(RangedValue::new(path.as_ref().to_path_buf(), source))
    }

    pub fn cli(path: impl AsRef<SystemPath>) -> Self {
        Self::new(path, ValueSource::Cli)
    }

    /// Returns the relative path as specified by the user.
    pub fn path(&self) -> &SystemPath {
        &self.0
    }

    pub fn source(&self) -> &ValueSource {
        self.0.source()
    }

    pub fn range(&self) -> Option<TextRange> {
        self.0.range()
    }

    /// Returns the owned relative path.
    pub fn into_path_buf(self) -> SystemPathBuf {
        self.0.into_inner()
    }

    /// Resolves the absolute path for `self` based on its origin.
    pub fn absolute_with_db(&self, db: &dyn Db) -> SystemPathBuf {
        self.absolute(db.project().root(db), db.system())
    }

    /// Resolves the absolute path for `self` based on its origin.
    pub fn absolute(&self, project_root: &SystemPath, system: &dyn System) -> SystemPathBuf {
        let relative_to = match &self.0.source() {
            ValueSource::File(_) => project_root,
            ValueSource::Cli => system.current_directory(),
        };

        SystemPath::absolute(&self.0, relative_to)
    }

    pub fn absolute_ranged(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
    ) -> RangedValue<SystemPathBuf> {
        let relative_to = match &self.0.source() {
            ValueSource::File(_) => project_root,
            ValueSource::Cli => system.current_directory(),
        };

        RangedValue::map_value(self.0.clone(), |path| {
            SystemPath::absolute(path, relative_to)
        })
    }
}

impl fmt::Display for RelativePathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Combine,
)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RelativeGlobPattern(RangedValue<String>);

impl RelativeGlobPattern {
    pub fn new(pattern: impl AsRef<str>, source: ValueSource) -> Self {
        Self(RangedValue::new(pattern.as_ref().to_string(), source))
    }

    pub fn cli(pattern: impl AsRef<str>) -> Self {
        Self::new(pattern, ValueSource::Cli)
    }

    pub fn source(&self) -> &ValueSource {
        self.0.source()
    }

    pub fn range(&self) -> Option<TextRange> {
        self.0.range()
    }

    /// Resolves the absolute pattern for `self` based on its origin.
    pub(crate) fn absolute(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
        kind: PortableGlobKind,
    ) -> Result<AbsolutePortableGlobPattern, PortableGlobError> {
        let relative_to = match &self.0.source() {
            ValueSource::File(_) => project_root,
            ValueSource::Cli => system.current_directory(),
        };

        let pattern = PortableGlobPattern::parse(&self.0, kind)?;
        Ok(pattern.into_absolute(relative_to))
    }
}

impl std::fmt::Display for RelativeGlobPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
