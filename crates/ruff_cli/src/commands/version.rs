use std::fmt::{Display, Formatter};
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use serde::Serialize;

use crate::args::HelpFormat;
use crate::build;

#[derive(Serialize)]
struct VersionMetadata {
    ruff_version: &'static str,
    rust_version: &'static str,
    build_time: &'static str,
    cargo_version: &'static str,
    build_os: &'static str,
    commit: &'static str,
    commit_time: &'static str,
    target: &'static str,
    dirty: bool,
    release: bool,
}

impl VersionMetadata {
    const fn from_build() -> Self {
        Self {
            ruff_version: build::PKG_VERSION,
            rust_version: build::RUST_VERSION,
            build_time: build::BUILD_TIME_3339,
            cargo_version: build::CARGO_VERSION,
            build_os: build::BUILD_OS,
            target: build::BUILD_TARGET,
            release: !build::TAG.is_empty(),
            commit: build::SHORT_COMMIT,
            commit_time: build::COMMIT_DATE_3339,
            dirty: !build::GIT_CLEAN,
        }
    }
}

impl Display for VersionMetadata {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "ruff {}", self.ruff_version)
    }
}

/// Display version information
pub(crate) fn version(output_format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    let metadata = VersionMetadata::from_build();

    let build_date = chrono::DateTime::parse_from_rfc3339(metadata.build_time)?;

    match output_format {
        HelpFormat::Text => {
            let status = if metadata.dirty {
                "-dirty"
            } else {
                if metadata.release {
                    ""
                } else {
                    "-dev"
                }
            };

            writeln!(
                stdout,
                "ruff {}{} ({} {})",
                &metadata.ruff_version,
                &status,
                &metadata.commit,
                build_date.format("%Y-%m-%d")
            )?;
            writeln!(stdout, "{}", &metadata.rust_version)?;
            writeln!(stdout, "{}", &metadata.cargo_version)?;
        }
        HelpFormat::Json => {
            serde_json::to_writer_pretty(stdout, &metadata)?;
        }
    };
    Ok(())
}
