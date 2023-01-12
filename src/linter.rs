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
use crate::autofix::fix_file;
use crate::checkers::ast::check_ast;
use crate::checkers::imports::check_imports;
use crate::checkers::lines::check_lines;
use crate::checkers::noqa::check_noqa;
use crate::checkers::tokens::check_tokens;
use crate::directives::Directives;
use crate::doc_lines::{doc_lines_from_ast, doc_lines_from_tokens};
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
use crate::registry::{Diagnostic, LintSource, RuleCode};
use crate::settings::{flags, Settings};
use crate::source_code::{Locator, Stylist};
use crate::{cache, directives, fix, fs, rustpython_helpers, violations};

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

/// Generate `Diagnostic`s from the source code contents at the
/// given `Path`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_path(
    path: &Path,
    package: Option<&Path>,
    contents: &str,
    tokens: Vec<LexResult>,
    locator: &Locator,
    stylist: &Stylist,
    directives: &Directives,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
) -> Result<Vec<Diagnostic>> {
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Aggregate all diagnostics.
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // Collect doc lines. This requires a rare mix of tokens (for comments) and AST
    // (for docstrings), which demands special-casing at this level.
    let use_doc_lines = settings.enabled.contains(&RuleCode::W505);
    let mut doc_lines = vec![];
    if use_doc_lines {
        doc_lines.extend(doc_lines_from_tokens(&tokens));
    }

    // Run the token-based rules.
    if settings
        .enabled
        .iter()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Tokens))
    {
        diagnostics.extend(check_tokens(locator, &tokens, settings, autofix));
    }

    // Run the AST-based rules.
    let use_ast = settings
        .enabled
        .iter()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::AST));
    let use_imports = !directives.isort.skip_file
        && settings
            .enabled
            .iter()
            .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Imports));
    if use_ast || use_imports || use_doc_lines {
        match rustpython_helpers::parse_program_tokens(tokens, "<filename>") {
            Ok(python_ast) => {
                if use_ast {
                    diagnostics.extend(check_ast(
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
                    diagnostics.extend(check_imports(
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
                if use_doc_lines {
                    doc_lines.extend(doc_lines_from_ast(&python_ast));
                }
            }
            Err(parse_error) => {
                if settings.enabled.contains(&RuleCode::E999) {
                    diagnostics.push(Diagnostic::new(
                        violations::SyntaxError(parse_error.error.to_string()),
                        Range::new(parse_error.location, parse_error.location),
                    ));
                }
            }
        }
    }

    // Deduplicate and reorder any doc lines.
    if use_doc_lines {
        doc_lines.sort_unstable();
        doc_lines.dedup();
    }

    // Run the lines-based rules.
    if settings
        .enabled
        .iter()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Lines))
    {
        diagnostics.extend(check_lines(
            contents,
            &directives.commented_lines,
            &doc_lines,
            settings,
            autofix,
        ));
    }

    // Enforce `noqa` directives.
    if matches!(noqa, flags::Noqa::Enabled)
        || settings
            .enabled
            .iter()
            .any(|rule_code| matches!(rule_code.lint_source(), LintSource::NoQA))
    {
        check_noqa(
            &mut diagnostics,
            contents,
            &directives.commented_lines,
            &directives.noqa_line_for,
            settings,
            autofix,
        );
    }

    // Create path ignores.
    if !diagnostics.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores)?;
        if !ignores.is_empty() {
            return Ok(diagnostics
                .into_iter()
                .filter(|diagnostic| !ignores.contains(&diagnostic.kind.code()))
                .collect());
        }
    }

    Ok(diagnostics)
}

const MAX_ITERATIONS: usize = 100;

/// Lint the source code at the given `Path`.
pub fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
    cache: flags::Cache,
    autofix: fix::FixMode,
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
        && matches!(autofix, fix::FixMode::None | fix::FixMode::Generate)
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
    let (messages, fixed) = if matches!(autofix, fix::FixMode::Apply | fix::FixMode::Diff) {
        let (transformed, fixed, messages) = lint_fix(&contents, path, package, settings)?;
        if fixed > 0 {
            if matches!(autofix, fix::FixMode::Apply) {
                write(path, transformed)?;
            } else if matches!(autofix, fix::FixMode::Diff) {
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
    let locator = Locator::new(&contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_contents(&contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));

    // Generate diagnostics, ignoring any existing `noqa` directives.
    let diagnostics = check_path(
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
        &diagnostics,
        &contents,
        &directives.noqa_line_for,
        &settings.external,
        stylist.line_ending(),
    )
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
    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Lint the inputs.
    let (messages, fixed) = if matches!(autofix, fix::FixMode::Apply | fix::FixMode::Diff) {
        let (transformed, fixed, messages) = lint_fix(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
        )?;

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

/// Generate `Diagnostic`s (optionally including any autofix
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
    let locator = Locator::new(contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_contents(contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));

    // Generate diagnostics.
    let diagnostics = check_path(
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

    // Convert from diagnostics to messages.
    let path_lossy = path.to_string_lossy();
    Ok(diagnostics
        .into_iter()
        .map(|diagnostic| {
            let source = if settings.show_source {
                Some(Source::from_diagnostic(&diagnostic, &locator))
            } else {
                None
            };
            Message::from_diagnostic(diagnostic, path_lossy.to_string(), source)
        })
        .collect())
}

/// Generate `Diagnostic`s from source code content, iteratively autofixing
/// until stable.
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
        let locator = Locator::new(&contents);

        // Detect the current code style (lazily).
        let stylist = Stylist::from_contents(&contents, &locator);

        // Extract the `# noqa` and `# isort: skip` directives from the source.
        let directives =
            directives::extract_directives(&tokens, directives::Flags::from_settings(settings));

        // Generate diagnostics.
        let diagnostics = check_path(
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
        if let Some((fixed_contents, applied)) = fix_file(&diagnostics, &locator) {
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
        let messages = diagnostics
            .into_iter()
            .map(|diagnostic| {
                let source = if settings.show_source {
                    Some(Source::from_diagnostic(&diagnostic, &locator))
                } else {
                    None
                };
                Message::from_diagnostic(diagnostic, path_lossy.to_string(), source)
            })
            .collect();
        return Ok((contents, fixed, messages));
    }
}

#[cfg(test)]
pub fn test_path(path: &Path, settings: &Settings) -> Result<Vec<Diagnostic>> {
    let contents = fs::read_file(path)?;
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
    let locator = Locator::new(&contents);
    let stylist = Stylist::from_contents(&contents, &locator);
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
    let mut diagnostics = check_path(
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
    )?;

    // Detect autofixes that don't converge after multiple iterations.
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.fix.is_some())
    {
        let max_iterations = 10;

        let mut contents = contents.clone();
        let mut iterations = 0;

        loop {
            let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
            let locator = Locator::new(&contents);
            let stylist = Stylist::from_contents(&contents, &locator);
            let directives =
                directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
            let diagnostics = check_path(
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
            )?;
            if let Some((fixed_contents, _)) = fix_file(&diagnostics, &locator) {
                if iterations < max_iterations {
                    iterations += 1;
                    contents = fixed_contents.to_string();
                } else {
                    panic!(
                        "Failed to converge after {max_iterations} iterations. This likely \
                         indicates a bug in the implementation of the fix."
                    );
                }
            } else {
                break;
            }
        }
    }

    diagnostics.sort_by_key(|diagnostic| diagnostic.location);
    Ok(diagnostics)
}
