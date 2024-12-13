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
use red_knot_python_semantic::PythonVersion;
use serde::Deserialize;

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct MarkdownTestConfig {
    pub(crate) environment: Option<Environment>,

    pub(crate) log: Option<Log>,
}

impl MarkdownTestConfig {
    pub(crate) fn from_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).context("Error while parsing Markdown TOML config")
    }

    pub(crate) fn python_version(&self) -> Option<PythonVersion> {
        self.environment.as_ref().and_then(|env| env.python_version)
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Environment {
    /// Python version to assume when resolving types.
    pub(crate) python_version: Option<PythonVersion>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum Log {
    /// Enable logging with tracing when `true`.
    Bool(bool),
    /// Enable logging and only show filters that match the given [env-filter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
    Filter(String),
}
