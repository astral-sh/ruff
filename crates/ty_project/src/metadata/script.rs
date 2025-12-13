use std::{io, process::Command, str::FromStr};

use camino::Utf8PathBuf;
use pep440_rs::VersionSpecifiers;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::script::ScriptTag;
use serde::Deserialize;
use thiserror::Error;

use crate::metadata::{
    pyproject::{Project, Tool},
    value::{RangedValue, ValueSource, ValueSourceGuard},
};

/// PEP 723 metadata as parsed from a `script` comment block.
///
/// See: <https://peps.python.org/pep-0723/>
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Pep723Metadata {
    pub dependencies: Option<RangedValue<Vec<toml::Value>>>,
    pub requires_python: Option<RangedValue<VersionSpecifiers>>,
    pub tool: Option<Tool>,

    /// The raw unserialized document.
    #[serde(skip)]
    pub raw: String,
}

#[derive(Debug, Error)]
pub enum Pep723Error {
    #[error(
        "An opening tag (`# /// script`) was found without a closing tag (`# ///`). Ensure that every line between the opening and closing tags (including empty lines) starts with a leading `#`."
    )]
    UnclosedBlock,
    #[error("The PEP 723 metadata block is missing from the script.")]
    MissingTag,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("Invalid filename `{0}` supplied")]
    InvalidFilename(String),
}

impl Pep723Metadata {
    /// Parse the PEP 723 metadata from `stdin`.
    pub fn from_script_str(
        contents: &[u8],
        source: ValueSource,
    ) -> Result<Option<Self>, Pep723Error> {
        let _guard = ValueSourceGuard::new(source, true);

        // Extract the `script` tag.
        let Some(ScriptTag { metadata, .. }) = ScriptTag::parse(contents) else {
            return Ok(None);
        };

        // Parse the metadata.
        Ok(Some(Self::from_str(&metadata)?))
    }

    pub fn to_project(&self) -> Project {
        Project {
            name: None,
            version: None,
            requires_python: self.requires_python.clone(),
        }
    }
}

/*
{
  "schema": {
    "version": "preview"
  },
  "target": "script",
  "script": {
    "path": "/Users/myuser/code/myproj/scripts/load-test.py"
  },
  "sync": {
    "environment": {
      "path": "/Users/myuser/.cache/uv/environments-v2/load-test-d6edaf5bfab110a8",
      "python": {
        "path": "/Users/myuser/.cache/uv/environments-v2/load-test-d6edaf5bfab110a8/bin/python3",
        "version": "3.14.0",
        "implementation": "cpython"
      }
    },
    "action": "check"
  },
  "lock": null,
  "dry_run": false
}
*/

/// The output of `uv sync --output-format=json --script ...`
#[derive(Debug, Clone, Deserialize)]
struct UvMetadata {
    sync: Option<UvSync>,
}

#[derive(Debug, Clone, Deserialize)]
struct UvSync {
    environment: Option<UvEnvironment>,
}

#[derive(Debug, Clone, Deserialize)]
struct UvEnvironment {
    path: Option<String>,
}

/// Ask `uv` to sync the script's venv to some temp dir so we can analyze dependencies properly
///
/// Returns the path to the venv on success
pub fn uv_sync_script(script_path: &SystemPath) -> Option<SystemPathBuf> {
    tracing::info!("Asking uv to sync the script's venv");
    let mut command = Command::new("uv");
    command
        .arg("sync")
        .arg("--output-format=json")
        .arg("--script")
        .arg(script_path.as_str());
    let output = command
        .output()
        .inspect_err(|e| {
            tracing::info!(
                "failed to run `uv sync --output-format=json --script {script_path}`: {e}"
            );
        })
        .ok()?;
    let metadata: UvMetadata = serde_json::from_slice(&output.stdout)
        .inspect_err(|e| {
            tracing::info!(
                "failed to parse `uv sync --output-format=json --script {script_path}`: {e}"
            );
        })
        .ok()?;
    let env_path = metadata.sync?.environment?.path?;
    let utf8_path = Utf8PathBuf::from(env_path);
    Some(SystemPathBuf::from_utf8_path_buf(utf8_path))
}

impl FromStr for Pep723Metadata {
    type Err = toml::de::Error;

    /// Parse `Pep723Metadata` from a raw TOML string.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let metadata = toml::from_str(raw)?;
        Ok(Self {
            raw: raw.to_string(),
            ..metadata
        })
    }
}
