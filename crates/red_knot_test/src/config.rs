//! TOML-deserializable Red Knot configuration, similar to `knot.toml`, to be able to
//! control some configuration options from Markdown files. For now, this supports the
//! following limited structure:
//!
//! ```toml
//! [environment]
//! python-version = "3.10"
//! ```

use anyhow::Context;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct MarkdownTestConfig {
    pub(crate) environment: Environment,
}

impl MarkdownTestConfig {
    pub(crate) fn from_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).context("Error while parsing Markdown TOML config")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Environment {
    pub(crate) python_version: String,
}
