use std::fmt;
use std::fmt::Formatter;
use std::hash::Hash;

use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_macros::Combine;
use ruff_ranged_value::{RangedValue, ValueSource};
use ruff_text_size::TextRange;

use crate::Db;
use crate::glob::{
    AbsolutePortableGlobPattern, PortableGlobError, PortableGlobKind, PortableGlobPattern,
};

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
    get_size2::GetSize,
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

    pub fn python_extension(path: impl AsRef<SystemPath>) -> Self {
        Self::new(path, ValueSource::Editor)
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
        let relative_to = match self.0.source() {
            ValueSource::File(_) => project_root,
            ValueSource::Cli | ValueSource::Editor => system.current_directory(),
        };

        // Expand tildes and environment variables in the path (e.g. `~/.cache/foo`).
        // Use `full_with_context` to route lookups through the `System` trait,
        // ensuring correct behavior in tests, WASM, and the LSP.
        let expanded = shellexpand::full_with_context(
            self.0.as_str(),
            || system.env_var("HOME").ok(),
            |var| match system.env_var(var) {
                Ok(val) => Ok(Some(val)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(e) => Err(e),
            },
        );

        match &expanded {
            Ok(path) => SystemPath::absolute(SystemPath::new(path.as_ref()), relative_to),
            Err(err) => {
                tracing::warn!(
                    "Failed to expand variables in path `{}`: {err}",
                    self.0.as_str()
                );
                SystemPath::absolute(&self.0, relative_to)
            }
        }
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
    get_size2::GetSize,
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

    /// Resolves the absolute pattern for `self` based on its origin.
    pub(crate) fn absolute(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
        kind: PortableGlobKind,
    ) -> Result<AbsolutePortableGlobPattern, PortableGlobError> {
        let relative_to = match self.0.source() {
            ValueSource::File(_) => project_root,
            ValueSource::Cli | ValueSource::Editor => system.current_directory(),
        };

        let pattern = PortableGlobPattern::parse(&self.0, kind)?;
        Ok(pattern.into_absolute(relative_to))
    }

    pub(crate) fn value(&self) -> &RangedValue<String> {
        &self.0
    }
}

impl std::fmt::Display for RelativeGlobPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ruff_db::system::{SystemPath, SystemPathBuf, TestSystem};
    use ruff_ranged_value::ValueSource;

    use super::RelativePathBuf;

    #[test]
    fn tilde_expansion_uses_system_env() {
        let system = TestSystem::default();
        system.set_env_var("HOME", "/test/home");

        let path = RelativePathBuf::new(
            "~/projects",
            ValueSource::File(Arc::new(SystemPathBuf::from("/project"))),
        );
        let resolved = path.absolute(SystemPath::new("/project"), &system);

        assert_eq!(resolved, SystemPathBuf::from("/test/home/projects"));
    }

    #[test]
    fn env_var_expansion_uses_system_env() {
        let system = TestSystem::default();
        system.set_env_var("MY_DIR", "/custom/dir");

        let path = RelativePathBuf::new(
            "$MY_DIR/sub",
            ValueSource::File(Arc::new(SystemPathBuf::from("/project"))),
        );
        let resolved = path.absolute(SystemPath::new("/project"), &system);

        assert_eq!(resolved, SystemPathBuf::from("/custom/dir/sub"));
    }

    #[test]
    fn undefined_env_var_falls_back_to_literal() {
        let system = TestSystem::default();

        let path = RelativePathBuf::new(
            "$NONEXISTENT/foo",
            ValueSource::File(Arc::new(SystemPathBuf::from("/project"))),
        );
        let resolved = path.absolute(SystemPath::new("/project"), &system);

        // When the variable is not found, the original path is used as a fallback.
        assert_eq!(resolved, SystemPathBuf::from("/project/$NONEXISTENT/foo"));
    }

    #[test]
    fn no_expansion_needed() {
        let system = TestSystem::default();

        let path = RelativePathBuf::new(
            "src/lib.rs",
            ValueSource::File(Arc::new(SystemPathBuf::from("/project"))),
        );
        let resolved = path.absolute(SystemPath::new("/project"), &system);

        assert_eq!(resolved, SystemPathBuf::from("/project/src/lib.rs"));
    }

    #[test]
    fn cli_source_resolves_relative_to_cwd() {
        let system = TestSystem::default();
        system.set_env_var("HOME", "/test/home");

        let path = RelativePathBuf::cli("~/config");
        let resolved = path.absolute(SystemPath::new("/project"), &system);

        assert_eq!(resolved, SystemPathBuf::from("/test/home/config"));
    }
}
