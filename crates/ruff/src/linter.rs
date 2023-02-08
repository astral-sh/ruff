use std::borrow::Cow;
use std::path::Path;

use anyhow::{anyhow, Result};
use colored::Colorize;
use log::error;
use rustpython_parser::error::ParseError;
use rustpython_parser::lexer::LexResult;

use crate::autofix::fix_file;
use crate::checkers::ast::check_ast;
use crate::checkers::filesystem::check_file_path;
use crate::checkers::imports::check_imports;
use crate::checkers::logical_lines::check_logical_lines;
use crate::checkers::noqa::check_noqa;
use crate::checkers::physical_lines::check_physical_lines;
use crate::checkers::tokens::check_tokens;
use crate::directives::Directives;
use crate::doc_lines::{doc_lines_from_ast, doc_lines_from_tokens};
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
use crate::registry::{Diagnostic, LintSource, Rule};
use crate::rules::pycodestyle;
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::{directives, fs, rustpython_helpers};

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A [`Result`]-like type that returns both data and an error. Used to return
/// diagnostics even in the face of parse errors, since many diagnostics can be
/// generated without a full AST.
pub struct LinterResult<T> {
    pub data: T,
    pub error: Option<ParseError>,
}

impl<T> LinterResult<T> {
    const fn new(data: T, error: Option<ParseError>) -> Self {
        Self { data, error }
    }

    fn map<U, F: FnOnce(T) -> U>(self, f: F) -> LinterResult<U> {
        LinterResult::new(f(self.data), self.error)
    }
}

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
) -> LinterResult<Vec<Diagnostic>> {
    // Aggregate all diagnostics.
    let mut diagnostics = vec![];
    let mut error = None;

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

    // Run the logical line-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::LogicalLines))
    {
        diagnostics.extend(check_logical_lines(&tokens, locator, stylist, settings));
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
        match rustpython_helpers::parse_program_tokens(tokens, &path.to_string_lossy()) {
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
                    pycodestyle::rules::syntax_error(&mut diagnostics, &parse_error);
                }
                error = Some(parse_error);
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
        .any(|rule_code| matches!(rule_code.lint_source(), LintSource::PhysicalLines))
    {
        diagnostics.extend(check_physical_lines(
            path,
            stylist,
            contents,
            indexer.commented_lines(),
            &doc_lines,
            settings,
            autofix,
        ));
    }

    // Ignore diagnostics based on per-file-ignores.
    if !diagnostics.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores);
        if !ignores.is_empty() {
            diagnostics.retain(|diagnostic| !ignores.contains(&diagnostic.kind.rule()));
        }
    };

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

    LinterResult::new(diagnostics, error)
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
    let LinterResult {
        data: diagnostics,
        error,
    } = check_path(
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
    );

    // Log any parse errors.
    if let Some(err) = error {
        error!(
            "{}{}{} {err:?}",
            "Failed to parse ".bold(),
            fs::relativize_path(path).bold(),
            ":".bold()
        );
    }

    // Add any missing `# noqa` pragmas.
    add_noqa(
        path,
        &diagnostics,
        &contents,
        indexer.commented_lines(),
        &directives.noqa_line_for,
        stylist.line_ending(),
    )
}

/// Generate a [`Message`] for each [`Diagnostic`] triggered by the given source
/// code.
pub fn lint_only(
    contents: &str,
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
    autofix: flags::Autofix,
) -> LinterResult<Vec<Message>> {
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
    let result = check_path(
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
    );

    // Convert from diagnostics to messages.
    let path_lossy = path.to_string_lossy();
    result.map(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| {
                let source = if settings.show_source {
                    Some(Source::from_diagnostic(&diagnostic, &locator))
                } else {
                    None
                };
                Message::from_diagnostic(diagnostic, path_lossy.to_string(), source)
            })
            .collect()
    })
}

/// Generate `Diagnostic`s from source code content, iteratively autofixing
/// until stable.
pub fn lint_fix<'a>(
    contents: &'a str,
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
) -> Result<(LinterResult<Vec<Message>>, Cow<'a, str>, usize)> {
    let mut transformed = Cow::Borrowed(contents);

    // Track the number of fixed errors across iterations.
    let mut fixed = 0;

    // As an escape hatch, bail after 100 iterations.
    let mut iterations = 0;

    // Track whether the _initial_ source code was parseable.
    let mut parseable = false;

    // Continuously autofix until the source code stabilizes.
    loop {
        // Tokenize once.
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&transformed);

        // Map row and column locations to byte slices (lazily).
        let locator = Locator::new(&transformed);

        // Detect the current code style (lazily).
        let stylist = Stylist::from_contents(&transformed, &locator);

        // Extra indices from the code.
        let indexer: Indexer = tokens.as_slice().into();

        // Extract the `# noqa` and `# isort: skip` directives from the source.
        let directives =
            directives::extract_directives(&tokens, directives::Flags::from_settings(settings));

        // Generate diagnostics.
        let result = check_path(
            path,
            package,
            &transformed,
            tokens,
            &locator,
            &stylist,
            &indexer,
            &directives,
            settings,
            flags::Autofix::Enabled,
            flags::Noqa::Enabled,
        );

        if iterations == 0 {
            parseable = result.error.is_none();
        } else {
            // If the source code was parseable on the first pass, but is no
            // longer parseable on a subsequent pass, then we've introduced a
            // syntax error. Return the original code.
            if parseable && result.error.is_some() {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!(
                        r#"
{}: Autofix introduced a syntax error. Reverting all changes.

This indicates a bug in `{}`. If you could open an issue at:

    {}/issues/new?title=%5BAutofix%20error%5D

...quoting the contents of `{}`, along with the `pyproject.toml` settings and executed command, we'd be very appreciative!
"#,
                        "error".red().bold(),
                        CARGO_PKG_NAME,
                        CARGO_PKG_REPOSITORY,
                        fs::relativize_path(path),
                    );
                }
                return Err(anyhow!("Autofix introduced a syntax error"));
            }
        }

        // Apply autofix.
        if let Some((fixed_contents, applied)) = fix_file(&result.data, &locator) {
            if iterations < MAX_ITERATIONS {
                // Count the number of fixed errors.
                fixed += applied;

                // Store the fixed contents.
                transformed = Cow::Owned(fixed_contents);

                // Increment the iteration count.
                iterations += 1;

                // Re-run the linter pass (by avoiding the break).
                continue;
            }

            #[allow(clippy::print_stderr)]
            {
                eprintln!(
                    r#"
{}: Failed to converge after {} iterations.

This indicates a bug in `{}`. If you could open an issue at:

    {}/issues/new?title=%5BInfinite%20loop%5D

...quoting the contents of `{}`, along with the `pyproject.toml` settings and executed command, we'd be very appreciative!
"#,
                    "error".red().bold(),
                    MAX_ITERATIONS,
                    CARGO_PKG_NAME,
                    CARGO_PKG_REPOSITORY,
                    fs::relativize_path(path),
                );
            }
        }

        // Convert to messages.
        let path_lossy = path.to_string_lossy();
        return Ok((
            result.map(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        let source = if settings.show_source {
                            Some(Source::from_diagnostic(&diagnostic, &locator))
                        } else {
                            None
                        };
                        Message::from_diagnostic(diagnostic, path_lossy.to_string(), source)
                    })
                    .collect()
            }),
            transformed,
            fixed,
        ));
    }
}
