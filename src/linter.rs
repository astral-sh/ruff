use std::fs::write;
use std::io;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

use anyhow::Result;
#[cfg(not(target_family = "wasm"))]
use log::debug;
use rustpython_parser::lexer::LexResult;

use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::autofix::fixer::fix_file;
use crate::checkers::ast::check_ast;
use crate::checkers::imports::check_imports;
use crate::checkers::lines::check_lines;
use crate::checkers::noqa::check_noqa;
use crate::checkers::tokens::check_tokens;
use crate::checks::{Check, CheckCode, CheckKind, LintSource};
use crate::code_gen::SourceGenerator;
use crate::directives::Directives;
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::{cache, directives, fs, rustpython_helpers};

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
    directives: &Directives,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
) -> Result<Vec<Check>> {
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
    let metadata = path.metadata()?;

    // Check the cache.
    if let Some(messages) = cache::get(path, &metadata, settings, autofix, cache) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(Diagnostics::new(messages));
    }

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Lint the file.
    let (contents, fixed, messages) = lint(contents, path, package, settings, autofix)?;

    // Re-populate the cache.
    cache::set(path, &metadata, settings, autofix, &messages, cache);

    // If we applied any fixes, write the contents back to disk.
    if fixed > 0 {
        write(path, contents)?;
    }

    Ok(Diagnostics { messages, fixed })
}

/// Add any missing `#noqa` pragmas to the source code at the given `Path`.
pub fn add_noqa_to_path(path: &Path, settings: &Settings) -> Result<usize> {
    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(&contents);

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
    )
}

/// Apply autoformatting to the source code at the given `Path`.
pub fn autoformat_path(path: &Path, _settings: &Settings) -> Result<()> {
    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

    // Generate the AST.
    let python_ast = rustpython_helpers::parse_program_tokens(tokens, "<filename>")?;
    let mut generator = SourceGenerator::default();
    generator.unparse_suite(&python_ast);
    write(path, generator.generate()?)?;

    Ok(())
}

/// Generate a list of `Check` violations from source code content derived from
/// stdin.
pub fn lint_stdin(
    path: Option<&Path>,
    package: Option<&Path>,
    stdin: &str,
    settings: &Settings,
    autofix: fixer::Mode,
) -> Result<Diagnostics> {
    // Read the file from disk.
    let contents = stdin.to_string();

    // Lint the file.
    let (contents, fixed, messages) = lint(
        contents,
        path.unwrap_or_else(|| Path::new("-")),
        package,
        settings,
        autofix,
    )?;

    // Write the fixed contents to stdout.
    if matches!(autofix, fixer::Mode::Apply) {
        io::stdout().write_all(contents.as_bytes())?;
    }

    Ok(Diagnostics { messages, fixed })
}

fn lint(
    mut contents: String,
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
    autofix: fixer::Mode,
) -> Result<(String, usize, Vec<Message>)> {
    // Track the number of fixed errors across iterations.
    let mut fixed = 0;

    // As an escape hatch, bail after 100 iterations.
    let mut iterations = 0;

    // Continuously autofix until the source code stabilizes.
    let messages = loop {
        // Tokenize once.
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);

        // Initialize the SourceCodeLocator (which computes offsets lazily).
        let locator = SourceCodeLocator::new(&contents);

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
            &directives,
            settings,
            autofix.into(),
            flags::Noqa::Enabled,
        )?;

        // Apply autofix.
        if matches!(autofix, fixer::Mode::Apply) && iterations < MAX_ITERATIONS {
            if let Some((fixed_contents, applied)) = fix_file(&checks, &locator) {
                // Count the number of fixed errors.
                fixed += applied;

                // Store the fixed contents.
                contents = fixed_contents.to_string();

                // Increment the iteration count.
                iterations += 1;

                // Re-run the linter pass (by avoiding the break).
                continue;
            }
        }

        // Convert to messages.
        let filename = path.to_string_lossy().to_string();
        break checks
            .into_iter()
            .map(|check| {
                let source = if settings.show_source {
                    Some(Source::from_check(&check, &locator))
                } else {
                    None
                };
                Message::from_check(check, filename.clone(), source)
            })
            .collect();
    };

    Ok((contents, fixed, messages))
}

#[cfg(test)]
pub fn test_path(path: &Path, settings: &Settings) -> Result<Vec<Check>> {
    let contents = fs::read_file(path)?;
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
    let locator = SourceCodeLocator::new(&contents);
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
        &directives,
        settings,
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    )
}
