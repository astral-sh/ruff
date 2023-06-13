use std::borrow::Cow;
use std::ops::Deref;
use std::path::Path;

use anyhow::{anyhow, Result};
use colored::Colorize;
use itertools::Itertools;
use log::error;
use rustc_hash::FxHashMap;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::ParseError;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::{Indexer, Locator, SourceFileBuilder, Stylist};
use ruff_python_stdlib::path::is_python_stub_file;

use crate::autofix::{fix_file, FixResult};
use crate::checkers::ast::check_ast;
use crate::checkers::filesystem::check_file_path;
use crate::checkers::imports::check_imports;
use crate::checkers::noqa::check_noqa;
use crate::checkers::physical_lines::check_physical_lines;
use crate::checkers::tokens::check_tokens;
use crate::directives::Directives;
use crate::doc_lines::{doc_lines_from_ast, doc_lines_from_tokens};
use crate::logging::DisplayParseError;
use crate::message::Message;
use crate::noqa::add_noqa;
use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle;
use crate::settings::{flags, Settings};
use crate::source_kind::SourceKind;
use crate::{directives, fs};

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

pub type FixTable = FxHashMap<Rule, usize>;

pub struct FixerResult<'a> {
    /// The result returned by the linter, after applying any fixes.
    pub result: LinterResult<(Vec<Message>, Option<ImportMap>)>,
    /// The resulting source code, after applying any fixes.
    pub transformed: Cow<'a, str>,
    /// The number of fixes applied for each [`Rule`].
    pub fixed: FixTable,
}

/// Generate `Diagnostic`s from the source code contents at the
/// given `Path`.
#[allow(clippy::too_many_arguments)]
pub fn check_path(
    path: &Path,
    package: Option<&Path>,
    tokens: Vec<LexResult>,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    directives: &Directives,
    settings: &Settings,
    noqa: flags::Noqa,
    source_kind: Option<&SourceKind>,
) -> LinterResult<(Vec<Diagnostic>, Option<ImportMap>)> {
    // Aggregate all diagnostics.
    let mut diagnostics = vec![];
    let mut imports = None;
    let mut error = None;

    // Collect doc lines. This requires a rare mix of tokens (for comments) and AST
    // (for docstrings), which demands special-casing at this level.
    let use_doc_lines = settings.rules.enabled(Rule::DocLineTooLong);
    let mut doc_lines = vec![];
    if use_doc_lines {
        doc_lines.extend(doc_lines_from_tokens(&tokens));
    }

    // Run the token-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| rule_code.lint_source().is_tokens())
    {
        let is_stub = is_python_stub_file(path);
        diagnostics.extend(check_tokens(locator, indexer, &tokens, settings, is_stub));
    }

    // Run the filesystem-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| rule_code.lint_source().is_filesystem())
    {
        diagnostics.extend(check_file_path(path, package, settings));
    }

    // Run the logical line-based rules.
    if settings
        .rules
        .iter_enabled()
        .any(|rule_code| rule_code.lint_source().is_logical_lines())
    {
        diagnostics.extend(crate::checkers::logical_lines::check_logical_lines(
            &tokens, locator, stylist, settings,
        ));
    }

    // Run the AST-based rules.
    let use_ast = settings
        .rules
        .iter_enabled()
        .any(|rule_code| rule_code.lint_source().is_ast());
    let use_imports = !directives.isort.skip_file
        && settings
            .rules
            .iter_enabled()
            .any(|rule_code| rule_code.lint_source().is_imports());
    if use_ast || use_imports || use_doc_lines {
        match ruff_rustpython::parse_program_tokens(tokens, &path.to_string_lossy()) {
            Ok(python_ast) => {
                if use_ast {
                    diagnostics.extend(check_ast(
                        &python_ast,
                        locator,
                        stylist,
                        indexer,
                        &directives.noqa_line_for,
                        settings,
                        noqa,
                        path,
                        package,
                    ));
                }
                if use_imports {
                    let (import_diagnostics, module_imports) = check_imports(
                        &python_ast,
                        locator,
                        indexer,
                        &directives.isort,
                        settings,
                        stylist,
                        path,
                        package,
                        source_kind,
                    );
                    imports = module_imports;
                    diagnostics.extend(import_diagnostics);
                }
                if use_doc_lines {
                    doc_lines.extend(doc_lines_from_ast(&python_ast, locator));
                }
            }
            Err(parse_error) => {
                // Always add a diagnostic for the syntax error, regardless of whether
                // `Rule::SyntaxError` is enabled. We avoid propagating the syntax error
                // if it's disabled via any of the usual mechanisms (e.g., `noqa`,
                // `per-file-ignores`), and the easiest way to detect that suppression is
                // to see if the diagnostic persists to the end of the function.
                pycodestyle::rules::syntax_error(&mut diagnostics, &parse_error, locator);
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
        .any(|rule_code| rule_code.lint_source().is_physical_lines())
    {
        diagnostics.extend(check_physical_lines(
            path, locator, stylist, indexer, &doc_lines, settings,
        ));
    }

    // Ignore diagnostics based on per-file-ignores.
    if !diagnostics.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores);
        if !ignores.is_empty() {
            diagnostics.retain(|diagnostic| !ignores.contains(diagnostic.kind.rule()));
        }
    };

    // Enforce `noqa` directives.
    if (noqa.into() && !diagnostics.is_empty())
        || settings
            .rules
            .iter_enabled()
            .any(|rule_code| rule_code.lint_source().is_noqa())
    {
        let ignored = check_noqa(
            &mut diagnostics,
            locator,
            indexer.comment_ranges(),
            &directives.noqa_line_for,
            error.is_none(),
            settings,
        );
        if noqa.into() {
            for index in ignored.iter().rev() {
                diagnostics.swap_remove(*index);
            }
        }
    }

    // If there was a syntax error, check if it should be discarded.
    if error.is_some() {
        // If the syntax error was removed by _any_ of the above disablement methods (e.g., a
        // `noqa` directive, or a `per-file-ignore`), discard it.
        if !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind.rule() == Rule::SyntaxError)
        {
            error = None;
        }

        // If the syntax error _diagnostic_ is disabled, discard the _diagnostic_.
        if !settings.rules.enabled(Rule::SyntaxError) {
            diagnostics.retain(|diagnostic| diagnostic.kind.rule() != Rule::SyntaxError);
        }
    }

    LinterResult::new((diagnostics, imports), error)
}

const MAX_ITERATIONS: usize = 100;

/// Add any missing `# noqa` pragmas to the source code at the given `Path`.
pub fn add_noqa_to_path(path: &Path, package: Option<&Path>, settings: &Settings) -> Result<usize> {
    // Read the file from disk.
    let contents = std::fs::read_to_string(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&contents);

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(&contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_tokens(&tokens, &locator);

    // Extra indices from the code.
    let indexer = Indexer::from_tokens(&tokens, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        directives::Flags::from_settings(settings),
        &locator,
        &indexer,
    );

    // Generate diagnostics, ignoring any existing `noqa` directives.
    let LinterResult {
        data: diagnostics,
        error,
    } = check_path(
        path,
        package,
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        settings,
        flags::Noqa::Disabled,
        None,
    );

    // Log any parse errors.
    if let Some(err) = error {
        // TODO(dhruvmanila): This should use `SourceKind`, update when
        // `--add-noqa` is supported for Jupyter notebooks.
        error!(
            "{}",
            DisplayParseError::new(err, locator.to_source_code(), None)
        );
    }

    // Add any missing `# noqa` pragmas.
    add_noqa(
        path,
        &diagnostics.0,
        &locator,
        indexer.comment_ranges(),
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
    noqa: flags::Noqa,
) -> LinterResult<(Vec<Message>, Option<ImportMap>)> {
    // Tokenize once.
    let tokens: Vec<LexResult> = ruff_rustpython::tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_tokens(&tokens, &locator);

    // Extra indices from the code.
    let indexer = Indexer::from_tokens(&tokens, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        directives::Flags::from_settings(settings),
        &locator,
        &indexer,
    );

    // Generate diagnostics.
    let result = check_path(
        path,
        package,
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        settings,
        noqa,
        None,
    );

    result.map(|(diagnostics, imports)| {
        (
            diagnostics_to_messages(diagnostics, path, &locator, &directives),
            imports,
        )
    })
}

/// Convert from diagnostics to messages.
fn diagnostics_to_messages(
    diagnostics: Vec<Diagnostic>,
    path: &Path,
    locator: &Locator,
    directives: &Directives,
) -> Vec<Message> {
    let file = once_cell::unsync::Lazy::new(|| {
        let mut builder =
            SourceFileBuilder::new(path.to_string_lossy().as_ref(), locator.contents());

        if let Some(line_index) = locator.line_index() {
            builder.set_line_index(line_index.clone());
        }

        builder.finish()
    });

    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let noqa_offset = directives.noqa_line_for.resolve(diagnostic.start());
            Message::from_diagnostic(diagnostic, file.deref().clone(), noqa_offset)
        })
        .collect()
}

/// Generate `Diagnostic`s from source code content, iteratively autofixing
/// until stable.
pub fn lint_fix<'a>(
    contents: &'a str,
    path: &Path,
    package: Option<&Path>,
    noqa: flags::Noqa,
    settings: &Settings,
    source_kind: &mut SourceKind,
) -> Result<FixerResult<'a>> {
    let mut transformed = Cow::Borrowed(contents);

    // Track the number of fixed errors across iterations.
    let mut fixed = FxHashMap::default();

    // As an escape hatch, bail after 100 iterations.
    let mut iterations = 0;

    // Track whether the _initial_ source code was parseable.
    let mut parseable = false;

    // Continuously autofix until the source code stabilizes.
    loop {
        // Tokenize once.
        let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&transformed);

        // Map row and column locations to byte slices (lazily).
        let locator = Locator::new(&transformed);

        // Detect the current code style (lazily).
        let stylist = Stylist::from_tokens(&tokens, &locator);

        // Extra indices from the code.
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // Extract the `# noqa` and `# isort: skip` directives from the source.
        let directives = directives::extract_directives(
            &tokens,
            directives::Flags::from_settings(settings),
            &locator,
            &indexer,
        );

        // Generate diagnostics.
        let result = check_path(
            path,
            package,
            tokens,
            &locator,
            &stylist,
            &indexer,
            &directives,
            settings,
            noqa,
            Some(source_kind),
        );

        if iterations == 0 {
            parseable = result.error.is_none();
        } else {
            // If the source code was parseable on the first pass, but is no
            // longer parseable on a subsequent pass, then we've introduced a
            // syntax error. Return the original code.
            if parseable && result.error.is_some() {
                report_autofix_syntax_error(
                    path,
                    &transformed,
                    &result.error.unwrap(),
                    fixed.keys().copied(),
                );
                return Err(anyhow!("Autofix introduced a syntax error"));
            }
        }

        // Apply autofix.
        if let Some(FixResult {
            code: fixed_contents,
            fixes: applied,
            source_map,
        }) = fix_file(&result.data.0, &locator)
        {
            if iterations < MAX_ITERATIONS {
                // Count the number of fixed errors.
                for (rule, count) in applied {
                    *fixed.entry(rule).or_default() += count;
                }

                if let SourceKind::Jupyter(notebook) = source_kind {
                    notebook.update(&source_map, &fixed_contents);
                }

                // Store the fixed contents.
                transformed = Cow::Owned(fixed_contents);

                // Increment the iteration count.
                iterations += 1;

                // Re-run the linter pass (by avoiding the break).
                continue;
            }

            report_failed_to_converge_error(path, &transformed, &result.data.0);
        }

        return Ok(FixerResult {
            result: result.map(|(diagnostics, imports)| {
                (
                    diagnostics_to_messages(diagnostics, path, &locator, &directives),
                    imports,
                )
            }),
            transformed,
            fixed,
        });
    }
}

fn collect_rule_codes(rules: impl IntoIterator<Item = Rule>) -> String {
    rules
        .into_iter()
        .map(|rule| rule.noqa_code().to_string())
        .sorted_unstable()
        .dedup()
        .join(", ")
}

#[allow(clippy::print_stderr)]
fn report_failed_to_converge_error(path: &Path, transformed: &str, diagnostics: &[Diagnostic]) {
    let codes = collect_rule_codes(diagnostics.iter().map(|diagnostic| diagnostic.kind.rule()));
    if cfg!(debug_assertions) {
        eprintln!(
            "{}: Failed to converge after {} iterations in `{}` with rule codes {}:---\n{}\n---",
            "debug error".red().bold(),
            MAX_ITERATIONS,
            fs::relativize_path(path),
            codes,
            transformed,
        );
    } else {
        eprintln!(
            r#"
{}: Failed to converge after {} iterations.

This indicates a bug in `{}`. If you could open an issue at:

    {}/issues/new?title=%5BInfinite%20loop%5D

...quoting the contents of `{}`, the rule codes {}, along with the `pyproject.toml` settings and executed command, we'd be very appreciative!
"#,
            "error".red().bold(),
            MAX_ITERATIONS,
            CARGO_PKG_NAME,
            CARGO_PKG_REPOSITORY,
            fs::relativize_path(path),
            codes
        );
    }
}

#[allow(clippy::print_stderr)]
fn report_autofix_syntax_error(
    path: &Path,
    transformed: &str,
    error: &ParseError,
    rules: impl IntoIterator<Item = Rule>,
) {
    let codes = collect_rule_codes(rules);
    if cfg!(debug_assertions) {
        eprintln!(
            "{}: Autofix introduced a syntax error in `{}` with rule codes {}: {}\n---\n{}\n---",
            "error".red().bold(),
            fs::relativize_path(path),
            codes,
            error,
            transformed,
        );
    } else {
        eprintln!(
            r#"
{}: Autofix introduced a syntax error. Reverting all changes.

This indicates a bug in `{}`. If you could open an issue at:

    {}/issues/new?title=%5BAutofix%20error%5D

...quoting the contents of `{}`, the rule codes {}, along with the `pyproject.toml` settings and executed command, we'd be very appreciative!
"#,
            "error".red().bold(),
            CARGO_PKG_NAME,
            CARGO_PKG_REPOSITORY,
            fs::relativize_path(path),
            codes,
        );
    }
}
