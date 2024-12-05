//! Ad-hoc implementation of a simplified TOML-deserializable Red Knot configuration, to
//! be able to control some configuration options from Markdown files. Eventually, this
//! should be replaced by the actual Red Knot config parsing. For now, this supports the
//! following TOML structure:
//!
//! ```toml
//! [tool.knot.environment]
//! target-version = "3.10"
//! ```

use anyhow::Context;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct MarkdownTestConfig {
    pub(crate) tool: Tool,
}

impl MarkdownTestConfig {
    pub(crate) fn from_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).context("Error while parsing Markdown TOML config")
    }
}

#[derive(Deserialize)]
pub(crate) struct Tool {
    pub(crate) knot: Knot,
}

#[derive(Deserialize)]
pub(crate) struct Knot {
    pub(crate) environment: Environment,
}

#[derive(Deserialize)]
pub(crate) struct Environment {
    #[serde(rename = "target-version")]
    pub(crate) target_version: String,
}
