use std::fs::write;
use std::io;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use log::debug;
use rustpython_parser::lexer::LexResult;
use similar::TextDiff;

use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::autofix::fixer::fix_file;
use crate::checkers::ast::check_ast;
use crate::checkers::imports::check_imports;
use crate::checkers::lines::check_lines;
use crate::checkers::noqa::check_noqa;
use crate::checkers::tokens::check_tokens;
use crate::checks::{Check, CheckCode, CheckKind, LintSource};
use crate::directives::Directives;
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::{cache, directives, fs, rustpython_helpers};

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

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

/// Generate a list of `Check` violations from the source code contents at the
/// given `Path`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_path(
    path: &Path,
    package: Option<&Path>,
    contents: &str,
    tokens: Vec<LexResult>,
    locator: &SourceCodeLocator,
    stylist: &SourceCodeStyleDetector,
    directives: &Directives,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
) -> Result<Vec<Check>> {
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Aggregate all checks.
    let mut checks: Vec<Check> = vec![];

    // Run the token-based checks.
    if settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::Tokens))
    {
        checks.extend(check_tokens(locator, &tokens, settings, autofix));
    }

    // Run the AST-based checks.
    let use_ast = settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::AST));
    let use_imports = settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::Imports));
    if use_ast || use_imports {
        match rustpython_helpers::parse_program_tokens(tokens, "<filename>") {
            Ok(python_ast) => {
                if use_ast {
                    checks.extend(check_ast(
                        &python_ast,
                        locator,
                        stylist,
                        &directives.noqa_line_for,
                        settings,
                        autofix,
                        noqa,
                        path,
                    ));
                }
                if use_imports {
                    checks.extend(check_imports(
                        &python_ast,
                        locator,
                        &directives.isort,
                        settings,
                        stylist,
                        autofix,
                        path,
                        package,
                    ));
                }
            }
            Err(parse_error) => {
                if settings.enabled.contains(&CheckCode::E999) {
                    checks.push(Check::new(
                        CheckKind::SyntaxError(parse_error.error.to_string()),
                        Range {
                            location: parse_error.location,
                            end_location: parse_error.location,
                        },
                    ));
                }
            }
        }
    }

    // Run the lines-based checks.
    if settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::Lines))
    {
        checks.extend(check_lines(
            contents,
            &directives.commented_lines,
            settings,
            autofix,
        ));
    }

    // Enforce `noqa` directives.
    if matches!(noqa, flags::Noqa::Enabled)
        || settings
            .enabled
            .iter()
            .any(|check_code| matches!(check_code.lint_source(), LintSource::NoQA))
    {
        check_noqa(
            &mut checks,
            contents,
            &directives.commented_lines,
            &directives.noqa_line_for,
            settings,
            autofix,
        );
    }

    // Create path ignores.
    if !checks.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores)?;
        if !ignores.is_empty() {
            return Ok(checks
                .into_iter()
                .filter(|check| !ignores.contains(&check.kind.code()))
                .collect());
        }
    }

    Ok(checks)
}

const MAX_ITERATIONS: usize = 100;

/// Lint the source code at the given `Path`.
pub fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
    cache: flags::Cache,
    autofix: fixer::Mode,
) -> Result<Diagnostics> {
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Check the cache.
    // TODO(charlie): `fixer::Mode::Apply` and `fixer::Mode::Diff` both have
    // side-effects that aren't captured in the cache. (In practice, it's fine
    // to cache `fixer::Mode::Apply`, since a file either has no fixes, or we'll
    // write the fixes to disk, thus invalidating the cache. But it's a bit hard
    // to reason about. We need to come up with a better solution here.)
    let metadata = if matches!(cache, flags::Cache::Enabled)
        && matches!(autofix, fixer::Mode::None | fixer::Mode::Generate)
    {
        let metadata = path.metadata()?;
        if let Some(messages) = cache::get(path, &metadata, settings, autofix.into()) {
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
    let (messages, fixed) = if matches!(autofix, fixer::Mode::Apply | fixer::Mode::Diff) {
        let (transformed, fixed, messages) = lint_fix(&contents, path, package, settings)?;
        if fixed > 0 {
            if matches!(autofix, fixer::Mode::Apply) {
                write(path, transformed)?;
            } else if matches!(autofix, fixer::Mode::Diff) {
                let mut stdout = io::stdout().lock();
                TextDiff::from_lines(&contents, &transformed)
                    .unified_diff()
                    .header(&fs::relativize_path(path), &fs::relativize_path(path))
                    .to_writer(&mut stdout)?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
        }
        (messages, fixed)
    } else {
        let messages = lint_only(&contents, path, package, settings, autofix.into())?;
        let fixed = 0;
        (messages, fixed)
    };

    // Re-populate the cache.
    if let Some(metadata) = metadata {
        cache::set(path, &metadata, settings, autofix.into(), &messages);
    }

    Ok(Diagnostics { messages, fixed })
}

/// Add any missing `#noqa` pragmas to the source code at the given `Path`.
pub fn add_noqa_to_path(path: &Path, settings: &Settings) -> Result<usize> {
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

    // Map row and column locations to byte slices (lazily).
    let locator = SourceCodeLocator::new(&contents);

    // Detect the current code style (lazily).
    let stylist = SourceCodeStyleDetector::from_contents(&contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        directives::Flags::from_settings(settings),
    );

    // Generate checks, ignoring any existing `noqa` directives.
    let checks = check_path(
        path,
        None,
        &contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        settings,
        flags::Autofix::Disabled,
        flags::Noqa::Disabled,
    )?;

    add_noqa(
        path,
        &checks,
        &contents,
        &directives.noqa_line_for,
        &settings.external,
        stylist.line_ending(),
    )
}

/// Generate a list of `Check` violations from source code content derived from
/// stdin.
pub fn lint_stdin(
    path: Option<&Path>,
    package: Option<&Path>,
    contents: &str,
    settings: &Settings,
    autofix: fixer::Mode,
) -> Result<Diagnostics> {
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Lint the inputs.
    let (messages, fixed) = if matches!(autofix, fixer::Mode::Apply | fixer::Mode::Diff) {
        let (transformed, fixed, messages) = lint_fix(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
        )?;

        if matches!(autofix, fixer::Mode::Apply) {
            // Write the contents to stdout, regardless of whether any errors were fixed.
            io::stdout().write_all(transformed.as_bytes())?;
        } else if matches!(autofix, fixer::Mode::Diff) {
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

        (messages, fixed)
    } else {
        let messages = lint_only(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
            autofix.into(),
        )?;
        let fixed = 0;
        (messages, fixed)
    };

    Ok(Diagnostics { messages, fixed })
}

/// Generate a list of `Check` violations (optionally including any autofix
/// patches) from source code content.
fn lint_only(
    contents: &str,
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Result<Vec<Message>> {
    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = SourceCodeLocator::new(contents);

    // Detect the current code style (lazily).
    let stylist = SourceCodeStyleDetector::from_contents(contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        directives::Flags::from_settings(settings),
    );

    // Generate checks.
    let checks = check_path(
        path,
        package,
        contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        settings,
        autofix,
        flags::Noqa::Enabled,
    )?;

    // Convert from checks to messages.
    let path_lossy = path.to_string_lossy();
    Ok(checks
        .into_iter()
        .map(|check| {
            let source = if settings.show_source {
                Some(Source::from_check(&check, &locator))
            } else {
                None
            };
            Message::from_check(check, path_lossy.to_string(), source)
        })
        .collect())
}

/// Generate a list of `Check` violations from source code content, iteratively
/// autofixing any violations until stable.
fn lint_fix(
    contents: &str,
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
) -> Result<(String, usize, Vec<Message>)> {
    let mut contents = contents.to_string();

    // Track the number of fixed errors across iterations.
    let mut fixed = 0;

    // As an escape hatch, bail after 100 iterations.
    let mut iterations = 0;

    // Continuously autofix until the source code stabilizes.
    loop {
        // Tokenize once.
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

        // Map row and column locations to byte slices (lazily).
        let locator = SourceCodeLocator::new(&contents);

        // Detect the current code style (lazily).
        let stylist = SourceCodeStyleDetector::from_contents(&contents, &locator);

        // Extract the `# noqa` and `# isort: skip` directives from the source.
        let directives = directives::extract_directives(
            &tokens,
            &locator,
            directives::Flags::from_settings(settings),
        );

        // Generate checks.
        let checks = check_path(
            path,
            package,
            &contents,
            tokens,
            &locator,
            &stylist,
            &directives,
            settings,
            flags::Autofix::Enabled,
            flags::Noqa::Enabled,
        )?;

        // Apply autofix.
        if let Some((fixed_contents, applied)) = fix_file(&checks, &locator) {
            if iterations < MAX_ITERATIONS {
                // Count the number of fixed errors.
                fixed += applied;

                // Store the fixed contents.
                contents = fixed_contents.to_string();

                // Increment the iteration count.
                iterations += 1;

                // Re-run the linter pass (by avoiding the break).
                continue;
            }

            eprintln!(
                "
{}: Failed to converge after {} iterations.

This likely indicates a bug in `{}`. If you could open an issue at:

{}/issues

quoting the contents of `{}`, along with the `pyproject.toml` settings and executed command, we'd \
                 be very appreciative!
",
                "warning".yellow().bold(),
                MAX_ITERATIONS,
                CARGO_PKG_NAME,
                CARGO_PKG_REPOSITORY,
                fs::relativize_path(path),
            );
        }

        // Convert to messages.
        let path_lossy = path.to_string_lossy();
        let messages = checks
            .into_iter()
            .map(|check| {
                let source = if settings.show_source {
                    Some(Source::from_check(&check, &locator))
                } else {
                    None
                };
                Message::from_check(check, path_lossy.to_string(), source)
            })
            .collect();
        return Ok((contents, fixed, messages));
    }
}

#[cfg(test)]
pub fn test_path(path: &Path, settings: &Settings) -> Result<Vec<Check>> {
    let contents = fs::read_file(path)?;
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
    let locator = SourceCodeLocator::new(&contents);
    let stylist = SourceCodeStyleDetector::from_contents(&contents, &locator);
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        directives::Flags::from_settings(settings),
    );
    check_path(
        path,
        None,
        &contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        settings,
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    )
}
