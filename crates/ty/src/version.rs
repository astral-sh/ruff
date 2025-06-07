//! Code for representing ty's release version number.
use std::fmt;

/// Information about the git repository where ty was built from.
pub(crate) struct CommitInfo {
    short_commit_hash: String,
    commit_date: String,
    commits_since_last_tag: u32,
    last_tag: Option<String>,
}

/// ty's version.
pub(crate) struct VersionInfo {
    /// ty's version, such as "0.5.1"
    version: String,
    /// Information about the git commit we may have been built from.
    ///
    /// `None` if not built from a git repo or if retrieval failed.
    commit_info: Option<CommitInfo>,
}

impl fmt::Display for VersionInfo {
    /// Formatted version information: `<version>[+<commits>] (<commit> <date>)`
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

impl From<VersionInfo> for clap::builder::Str {
    fn from(val: VersionInfo) -> Self {
        val.to_string().into()
    }
}

/// Returns information about ty's version.
pub(crate) fn version() -> VersionInfo {
    // Environment variables are only read at compile-time
    macro_rules! option_env_str {
        ($name:expr) => {
            option_env!($name).map(|s| s.to_string())
        };
    }

    // Commit info is pulled from git and set by `build.rs`
    let commit_info = option_env_str!("TY_COMMIT_SHORT_HASH").map(|short_commit_hash| CommitInfo {
        short_commit_hash,
        commit_date: option_env_str!("TY_COMMIT_DATE").unwrap(),
        commits_since_last_tag: option_env_str!("TY_LAST_TAG_DISTANCE")
            .as_deref()
            .map_or(0, |value| value.parse::<u32>().unwrap_or(0)),
        last_tag: option_env_str!("TY_LAST_TAG"),
    });

    // The version is pulled from `dist-workspace.toml` and set by `build.rs`
    let version = option_env_str!("TY_VERSION").unwrap_or_else(|| {
        // If missing, using the last tag
        commit_info
            .as_ref()
            .and_then(|info| {
                info.last_tag.as_ref().map(|tag| {
                    tag.strip_prefix("v")
                        .map(std::string::ToString::to_string)
                        .unwrap_or(tag.clone())
                })
            })
            .unwrap_or("unknown".to_string())
    });

    VersionInfo {
        version,
        commit_info,
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::{CommitInfo, VersionInfo};

    #[test]
    fn version_formatting() {
        let version = VersionInfo {
            version: "0.0.0".to_string(),
            commit_info: None,
        };
        assert_snapshot!(version, @"0.0.0");
    }

    #[test]
    fn version_formatting_with_commit_info() {
        let version = VersionInfo {
            version: "0.0.0".to_string(),
            commit_info: Some(CommitInfo {
                short_commit_hash: "53b0f5d92".to_string(),
                commit_date: "2023-10-19".to_string(),
                commits_since_last_tag: 0,
                last_tag: None,
            }),
        };
        assert_snapshot!(version, @"0.0.0 (53b0f5d92 2023-10-19)");
    }

    #[test]
    fn version_formatting_with_commits_since_last_tag() {
        let version = VersionInfo {
            version: "0.0.0".to_string(),
            commit_info: Some(CommitInfo {
                short_commit_hash: "53b0f5d92".to_string(),
                commit_date: "2023-10-19".to_string(),
                commits_since_last_tag: 24,
                last_tag: None,
            }),
        };
        assert_snapshot!(version, @"0.0.0+24 (53b0f5d92 2023-10-19)");
    }
}
