//! TOML-deserializable Red Knot configuration, similar to `knot.toml`, to be able to
//! control some configuration options from Markdown files. For now, this supports the
//! following limited structure:
//!
//! ```toml
//! log = true # or log = "red_knot=WARN"
//! [environment]
//! python-version = "3.10"
//! ```

use anyhow::Context;
use red_knot_python_semantic::PythonPlatform;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct MarkdownTestConfig {
    pub(crate) environment: Option<Environment>,

    pub(crate) log: Option<Log>,

    /// The [`ruff_db::system::System`] to use for tests.
    ///
    /// Defaults to the case-sensitive [`ruff_db::system::InMemorySystem`].
    pub(crate) system: Option<SystemKind>,
}

impl MarkdownTestConfig {
    pub(crate) fn from_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).context("Error while parsing Markdown TOML config")
    }

    pub(crate) fn python_version(&self) -> Option<PythonVersion> {
        self.environment.as_ref().and_then(|env| env.python_version)
    }

    pub(crate) fn python_platform(&self) -> Option<PythonPlatform> {
        self.environment
            .as_ref()
            .and_then(|env| env.python_platform.clone())
    }

    pub(crate) fn typeshed(&self) -> Option<&SystemPath> {
        self.environment
            .as_ref()
            .and_then(|env| env.typeshed.as_deref())
    }

    pub(crate) fn extra_paths(&self) -> Option<&[SystemPathBuf]> {
        self.environment
            .as_ref()
            .and_then(|env| env.extra_paths.as_deref())
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Environment {
    /// Target Python version to assume when resolving types.
    pub(crate) python_version: Option<PythonVersion>,

    /// Target platform to assume when resolving types.
    pub(crate) python_platform: Option<PythonPlatform>,

    /// Path to a custom typeshed directory.
    pub(crate) typeshed: Option<SystemPathBuf>,

    /// Additional search paths to consider when resolving modules.
    pub(crate) extra_paths: Option<Vec<SystemPathBuf>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum Log {
    /// Enable logging with tracing when `true`.
    Bool(bool),
    /// Enable logging and only show filters that match the given [env-filter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
    Filter(String),
}

/// The system to use for tests.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SystemKind {
    /// Use an in-memory system with a case sensitive file system..
    ///
    /// This is recommended for all tests because it's fast.
    #[default]
    InMemory,

    /// Use the os system.
    ///
    /// This system should only be used when testing system or OS specific behavior.
    Os,
}
