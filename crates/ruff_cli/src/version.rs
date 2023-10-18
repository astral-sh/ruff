//! Code for representing Ruff's release version number.
use serde::Serialize;
use std::fmt;

/// Information about the git repository where Ruff was built from.
#[derive(Serialize)]
pub(crate) struct CommitInfo {
    short_commit_hash: String,
    commit_hash: String,
    commit_date: String,
    last_tag: String,
    commits_since_last_tag: u32,
}

/// Ruff's version.
#[derive(Serialize)]
pub(crate) struct VersionInfo {
    /// Ruff's version, such as "0.5.1"
    version: String,
    /// Information about the git commit we may have been built from.
    ///
    /// `None` if not built from a git repo or if retrieval failed.
    commit_info: Option<CommitInfo>,
}

impl fmt::Display for VersionInfo {
    /// Formatted version information: "<version>[+<commits>] (<commit> <date>)"
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)?;

        if let Some(ref ci) = self.commit_info {
            if ci.commits_since_last_tag > 0 {
                write!(f, "+{}", ci.commits_since_last_tag)?;
            }
            write!(f, " ({} {})", ci.short_commit_hash, ci.commit_date)?;
        }

        Ok(())
    }
}

/// Returns information about Ruff's version.
pub(crate) fn version() -> VersionInfo {
    macro_rules! option_env_str {
        ($name:expr) => {
            option_env!($name).map(|s| s.to_string())
        };
    }

    // This version is pulled from Cargo.toml and set by Cargo
    let version = option_env_str!("CARGO_PKG_VERSION").unwrap();

    // Commit info is pulled from git and set by `build.rs`
    let commit_info = option_env_str!("RUFF_COMMIT_HASH").map(|commit_hash| CommitInfo {
        short_commit_hash: option_env_str!("RUFF_COMMIT_SHORT_HASH").unwrap(),
        commit_hash,
        commit_date: option_env_str!("RUFF_COMMIT_DATE").unwrap(),
        last_tag: option_env_str!("RUFF_LAST_TAG").unwrap(),
        commits_since_last_tag: option_env_str!("RUFF_LAST_TAG_DISTANCE")
            .unwrap()
            .parse::<u32>()
            .unwrap(),
    });

    VersionInfo {
        version,
        commit_info,
    }
}
