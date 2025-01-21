use ruff_db::system::{System, SystemPath, SystemPathBuf};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::combine::Combine;
use crate::Db;

#[derive(Clone, Debug)]
pub enum ValueSource {
    /// Value loaded from a project's configuration file.
    ///
    /// Ideally, we'd use [`ruff_db::files::File`] but we can't because the database hasn't been
    /// created when loading the configuration.
    File(Arc<SystemPathBuf>),
    /// The value comes from a CLI argument, while it's left open if specified using a short argument,
    /// long argument (`--extra-paths`) or `--config key=value`.
    Cli,
}

impl fmt::Display for ValueSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(p) => fmt::Display::fmt(p, f),
            Self::Cli => write!(f, "--config cli option"),
        }
    }
}

thread_local! {
    /// Serde doesn't provide any easy means to pass a value to a [`Deserialize`] implementation,
    /// but we want to associate each deserialized [`RelativePath`] with the source from
    /// where it origins. We use a thread local variable to work around this limitation.
    ///
    /// Use the [`ValueSourceGuard`] to initialize the thread local before calling into any
    /// deserialization code. It ensures that the thread local variable gets cleaned up
    /// once deserialization is done (once the guard gets dropped).
    static VALUE_SOURCE: RefCell<Option<ValueSource>> = const { RefCell::new(None) };
}

/// Guard to safely change the [`VALUE_SOURCE`] for the current thread.
#[must_use]
pub(super) struct ValueSourceGuard {
    prev_value: Option<ValueSource>,
}

impl ValueSourceGuard {
    pub(super) fn new(source: ValueSource) -> Self {
        let prev = VALUE_SOURCE.replace(Some(source));
        Self { prev_value: prev }
    }
}

impl Drop for ValueSourceGuard {
    fn drop(&mut self) {
        VALUE_SOURCE.set(self.prev_value.take());
    }
}

/// A possibly relative path in a configuration file.
///
/// Relative paths in configuration files or from CLI options
/// require different anchoring:
///
/// * CLI: The path is relative to the current working directory
/// * Configuration file: The path is relative to the project's root.
#[derive(Debug, Clone)]
pub struct RelativePathBuf {
    path: SystemPathBuf,
    source: ValueSource,
}

impl RelativePathBuf {
    pub fn new(path: impl AsRef<SystemPath>, source: ValueSource) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn cli(path: impl AsRef<SystemPath>) -> Self {
        Self::new(path, ValueSource::Cli)
    }

    /// Returns the relative path as specified by the user.
    pub fn path(&self) -> &SystemPath {
        &self.path
    }

    /// Returns the owned relative path.
    pub fn into_path_buf(self) -> SystemPathBuf {
        self.path
    }

    /// Resolves the absolute path for `self` based on from where the value origins.
    pub fn absolute_with_db(&self, db: &dyn Db) -> SystemPathBuf {
        self.absolute(db.project().root(db), db.system())
    }

    /// Resolves the absolute path for `self` based on from where the value origins.
    pub fn absolute(&self, project_root: &SystemPath, system: &dyn System) -> SystemPathBuf {
        let relative_to = match &self.source {
            ValueSource::File(_) => project_root,
            ValueSource::Cli => system.current_directory(),
        };

        SystemPath::absolute(&self.path, relative_to)
    }
}

// TODO(micha): Derive most of those implementations once `RelativePath` uses `Value`.
//   and use `serde(transparent, deny_unknown_fields)`
impl Combine for RelativePathBuf {
    fn combine(self, _other: Self) -> Self {
        self
    }

    #[inline(always)]
    fn combine_with(&mut self, _other: Self) {}
}

impl Hash for RelativePathBuf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl PartialEq for RelativePathBuf {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for RelativePathBuf {}

impl PartialOrd for RelativePathBuf {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RelativePathBuf {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

impl Serialize for RelativePathBuf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.path.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RelativePathBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = SystemPathBuf::deserialize(deserializer)?;
        Ok(VALUE_SOURCE.with_borrow(|source| {
            let source = source
                .clone()
                .expect("Thread local `VALUE_SOURCE` to be set. Use `ValueSourceGuard` to set the value source before calling serde/toml `from_str`.");

            Self { path, source }
        }))
    }
}
