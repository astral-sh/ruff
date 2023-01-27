use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use rustpython_parser::lexer::LexResult;

use crate::ast::types::Range;
use crate::autofix::fix_file;
use crate::checkers::ast::check_ast;
use crate::checkers::filesystem::check_file_path;
use crate::checkers::imports::check_imports;
use crate::checkers::lines::check_lines;
use crate::checkers::noqa::check_noqa;
use crate::checkers::tokens::check_tokens;
use crate::directives::Directives;
use crate::doc_lines::{doc_lines_from_ast, doc_lines_from_tokens};
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
#[cfg(test)]
use crate::packaging::detect_package_root;
use crate::registry::{Diagnostic, LintSource, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::{directives, fs, rustpython_helpers, violations};

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Generate `Diagnostic`s from the source code contents at the
/// given `Path`.
#[allow(clippy::too_many_arguments)]
pub fn check_path(
    path: &Path,
    package: Option<&Path>,
    contents: &str,
    tokens: Vec<LexResult>,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    directives: &Directives,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
) -> Result<Vec<Diagnostic>> {
    // Aggregate all diagnostics.
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // Collect doc lines. This requires a rare mix of tokens (for comments) and AST
    // (for docstrings), which demands special-casing at this level.
    let use_doc_lines = settings.rules.enabled(&Rule::DocLineTooLong);
    let mut doc_lines = vec![];
    if use_doc_lines {
        doc_lines.extend(doc_lines_from_tokens(&tokens));
    }

    // Run the token-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Tokens))
    {
        diagnostics.extend(check_tokens(locator, &tokens, settings, autofix));
    }

    // Run the filesystem-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Filesystem))
    {
        diagnostics.extend(check_file_path(path, package, settings));
    }

    // Run the AST-based rules.
    let use_ast = settings
        .rules
        .iter_enabled()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Ast));
    let use_imports = !directives.isort.skip_file
        && settings
            .rules
            .iter_enabled()
            .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Imports));
    if use_ast || use_imports || use_doc_lines {
        match rustpython_helpers::parse_program_tokens(tokens, "<filename>") {
            Ok(python_ast) => {
                if use_ast {
                    diagnostics.extend(check_ast(
                        &python_ast,
                        locator,
                        stylist,
                        indexer,
                        &directives.noqa_line_for,
                        settings,
                        autofix,
                        noqa,
                        path,
                        package,
                    ));
                }
                if use_imports {
                    diagnostics.extend(check_imports(
                        &python_ast,
                        locator,
                        indexer,
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
                if settings.rules.enabled(&Rule::SyntaxError) {
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
        .rules
        .iter_enabled()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Lines))
    {
        diagnostics.extend(check_lines(
            path,
            stylist,
            contents,
            indexer.commented_lines(),
            &doc_lines,
            settings,
            autofix,
        ));
    }

    // Enforce `noqa` directives.
    if (matches!(noqa, flags::Noqa::Enabled) && !diagnostics.is_empty())
        || settings
            .rules
            .iter_enabled()
            .any(|rule_code| matches!(rule_code.lint_source(), LintSource::NoQa))
    {
        check_noqa(
            &mut diagnostics,
            contents,
            indexer.commented_lines(),
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
                .filter(|diagnostic| !ignores.contains(&diagnostic.kind.rule()))
                .collect());
        }
    }

    Ok(diagnostics)
}

const MAX_ITERATIONS: usize = 100;

/// Add any missing `#noqa` pragmas to the source code at the given `Path`.
pub fn add_noqa_to_path(path: &Path, settings: &Settings) -> Result<usize> {
    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(&contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_contents(&contents, &locator);

    // Extra indices from the code.
    let indexer: Indexer = tokens.as_slice().into();

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
        &indexer,
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

/// Generate `Diagnostic`s (optionally including any autofix
/// patches) from source code content.
pub fn lint_only(
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

    // Extra indices from the code.
    let indexer: Indexer = tokens.as_slice().into();

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
        &indexer,
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
pub fn lint_fix(
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

        // Extra indices from the code.
        let indexer: Indexer = tokens.as_slice().into();

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
            &indexer,
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
                r#"
{}: Failed to converge after {} iterations.

This likely indicates a bug in `{}`. If you could open an issue at:

{}/issues/new?title=%5BInfinite%20loop%5D

quoting the contents of `{}`, along with the `pyproject.toml` settings and executed command, we'd be very appreciative!
"#,
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
    let indexer: Indexer = tokens.as_slice().into();
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
    let mut diagnostics = check_path(
        path,
        path.parent()
            .and_then(|parent| detect_package_root(parent, &settings.namespace_packages)),
        &contents,
        tokens,
        &locator,
        &stylist,
        &indexer,
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
            let indexer: Indexer = tokens.as_slice().into();
            let directives =
                directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
            let diagnostics = check_path(
                path,
                None,
                &contents,
                tokens,
                &locator,
                &stylist,
                &indexer,
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
