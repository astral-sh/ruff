#![cfg_attr(target_family = "wasm", allow(dead_code))]
use std::fs::write;
use std::io;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use log::{debug, error};
use ruff::linter::{lint_fix, lint_only, LinterResult};
use ruff::message::Message;
use ruff::settings::{flags, AllSettings, Settings};
use ruff::{fix, fs};
use similar::TextDiff;

use crate::cache;

#[derive(Debug, Default)]
pub struct Diagnostics {
    pub messages: Vec<Message>,
    pub fixed: usize,
}

impl Diagnostics {
    pub fn new(messages: Vec<Message>) -> Self {
        Self { messages, fixed: 0 }
    }
}

impl AddAssign for Diagnostics {
    fn add_assign(&mut self, other: Self) {
        self.messages.extend(other.messages);
        self.fixed += other.fixed;
    }
}

/// Lint the source code at the given `Path`.
pub fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &AllSettings,
    cache: flags::Cache,
    autofix: fix::FixMode,
) -> Result<Diagnostics> {
    // Check the cache.
    // TODO(charlie): `fixer::Mode::Apply` and `fixer::Mode::Diff` both have
    // side-effects that aren't captured in the cache. (In practice, it's fine
    // to cache `fixer::Mode::Apply`, since a file either has no fixes, or we'll
    // write the fixes to disk, thus invalidating the cache. But it's a bit hard
    // to reason about. We need to come up with a better solution here.)
    let metadata = if matches!(cache, flags::Cache::Enabled)
        && matches!(autofix, fix::FixMode::None | fix::FixMode::Generate)
    {
        let metadata = path.metadata()?;
        if let Some(messages) =
            cache::get(path, package.as_ref(), &metadata, settings, autofix.into())
        {
            debug!("Cache hit for: {}", path.to_string_lossy());
            return Ok(Diagnostics::new(messages));
        }
        Some(metadata)
    } else {
        None
    };

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Lint the file.
    let (
        LinterResult {
            data: messages,
            error: parse_error,
        },
        fixed,
    ) = if matches!(autofix, fix::FixMode::Apply | fix::FixMode::Diff) {
        if let Ok((result, transformed, fixed)) = lint_fix(&contents, path, package, &settings.lib)
        {
            if fixed > 0 {
                if matches!(autofix, fix::FixMode::Apply) {
                    write(path, transformed.as_bytes())?;
                } else if matches!(autofix, fix::FixMode::Diff) {
                    let mut stdout = io::stdout().lock();
                    TextDiff::from_lines(contents.as_str(), &transformed)
                        .unified_diff()
                        .header(&fs::relativize_path(path), &fs::relativize_path(path))
                        .to_writer(&mut stdout)?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
            }
            (result, fixed)
        } else {
            // If we fail to autofix, lint the original source code.
            let result = lint_only(&contents, path, package, &settings.lib, autofix.into());
            let fixed = 0;
            (result, fixed)
        }
    } else {
        let result = lint_only(&contents, path, package, &settings.lib, autofix.into());
        let fixed = 0;
        (result, fixed)
    };

    if let Some(err) = parse_error {
        // Notify the user of any parse errors.
        error!(
            "{}{}{} {err}",
            "Failed to parse ".bold(),
            fs::relativize_path(path).bold(),
            ":".bold()
        );

        // Purge the cache.
        cache::del(path, package.as_ref(), settings, autofix.into());
    } else {
        // Re-populate the cache.
        if let Some(metadata) = metadata {
            cache::set(
                path,
                package.as_ref(),
                &metadata,
                settings,
                autofix.into(),
                &messages,
            );
        }
    }

    Ok(Diagnostics { messages, fixed })
}

/// Generate `Diagnostic`s from source code content derived from
/// stdin.
pub fn lint_stdin(
    path: Option<&Path>,
    package: Option<&Path>,
    contents: &str,
    settings: &Settings,
    autofix: fix::FixMode,
) -> Result<Diagnostics> {
    // Lint the inputs.
    let (
        LinterResult {
            data: messages,
            error: parse_error,
        },
        fixed,
    ) = if matches!(autofix, fix::FixMode::Apply | fix::FixMode::Diff) {
        if let Ok((result, transformed, fixed)) = lint_fix(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
        ) {
            if matches!(autofix, fix::FixMode::Apply) {
                // Write the contents to stdout, regardless of whether any errors were fixed.
                io::stdout().write_all(transformed.as_bytes())?;
            } else if matches!(autofix, fix::FixMode::Diff) {
                // But only write a diff if it's non-empty.
                if fixed > 0 {
                    let text_diff = TextDiff::from_lines(contents, &transformed);
                    let mut unified_diff = text_diff.unified_diff();
                    if let Some(path) = path {
                        unified_diff.header(&fs::relativize_path(path), &fs::relativize_path(path));
                    }

                    let mut stdout = io::stdout().lock();
                    unified_diff.to_writer(&mut stdout)?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
            }

            (result, fixed)
        } else {
            // If we fail to autofix, lint the original source code.
            let result = lint_only(
                contents,
                path.unwrap_or_else(|| Path::new("-")),
                package,
                settings,
                autofix.into(),
            );
            let fixed = 0;

            // Write the contents to stdout anyway.
            if matches!(autofix, fix::FixMode::Apply) {
                io::stdout().write_all(contents.as_bytes())?;
            }

            (result, fixed)
        }
    } else {
        let result = lint_only(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
            autofix.into(),
        );
        let fixed = 0;
        (result, fixed)
    };

    if let Some(err) = parse_error {
        error!(
            "Failed to parse {}: {err}",
            path.map_or_else(|| "-".into(), fs::relativize_path).bold()
        );
    }

    Ok(Diagnostics { messages, fixed })
}
