#![cfg(test)]

/// Helper functions for the tests of rule implementations.
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use ruff_diagnostics::Diagnostic;
use rustc_hash::FxHashMap;
use rustpython_parser::lexer::LexResult;

use ruff_python_ast::source_code::{Indexer, Locator, SourceFileBuilder, Stylist};

use crate::autofix::fix_file;
use crate::directives;
use crate::linter::{check_path, LinterResult};
use crate::message::{Emitter, EmitterContext, Message, TextEmitter};
use crate::packaging::detect_package_root;
use crate::rules::pycodestyle::rules::syntax_error;
use crate::settings::{flags, Settings};

pub fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
    Path::new("./resources/test/").join(path)
}

/// A convenient wrapper around [`check_path`], that additionally
/// asserts that autofixes converge after 10 iterations.
pub fn test_path(path: impl AsRef<Path>, settings: &Settings) -> Result<Vec<Message>> {
    static MAX_ITERATIONS: usize = 10;

    let path = test_resource_path("fixtures").join(path);
    let contents = std::fs::read_to_string(&path)?;
    let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&contents);
    let locator = Locator::new(&contents);
    let stylist = Stylist::from_tokens(&tokens, &locator);
    let indexer = Indexer::from_tokens(&tokens, &locator);
    let directives = directives::extract_directives(
        &tokens,
        directives::Flags::from_settings(settings),
        &locator,
        &indexer,
    );
    let LinterResult {
        data: (diagnostics, _imports),
        error,
    } = check_path(
        &path,
        path.parent()
            .and_then(|parent| detect_package_root(parent, &settings.namespace_packages)),
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        settings,
        flags::Noqa::Enabled,
        flags::Autofix::Enabled,
    );

    let source_has_errors = error.is_some();

    // Detect autofixes that don't converge after multiple iterations.
    let mut iterations = 0;

    if diagnostics
        .iter()
        .any(|diagnostic| !diagnostic.fix.is_empty())
    {
        let mut diagnostics = diagnostics.clone();
        let mut contents = contents.clone();

        while let Some((fixed_contents, _)) = fix_file(&diagnostics, &Locator::new(&contents)) {
            if iterations < MAX_ITERATIONS {
                iterations += 1;
            } else {
                let output = print_diagnostics(diagnostics, &path, &contents);

                panic!(
                        "Failed to converge after {MAX_ITERATIONS} iterations. This likely \
                         indicates a bug in the implementation of the fix. Last diagnostics:\n{output}"
                    );
            }

            let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&fixed_contents);
            let locator = Locator::new(&fixed_contents);
            let stylist = Stylist::from_tokens(&tokens, &locator);
            let indexer = Indexer::from_tokens(&tokens, &locator);
            let directives = directives::extract_directives(
                &tokens,
                directives::Flags::from_settings(settings),
                &locator,
                &indexer,
            );

            let LinterResult {
                data: (fixed_diagnostics, _),
                error: fixed_error,
            } = check_path(
                &path,
                None,
                tokens,
                &locator,
                &stylist,
                &indexer,
                &directives,
                settings,
                flags::Noqa::Enabled,
                flags::Autofix::Enabled,
            );

            if let Some(fixed_error) = fixed_error {
                if !source_has_errors {
                    // Previous fix introduced a syntax error, abort
                    let fixes = print_diagnostics(diagnostics, &path, &contents);

                    let mut syntax_diagnostics = Vec::new();
                    syntax_error(&mut syntax_diagnostics, &fixed_error, &locator);
                    let syntax_errors =
                        print_diagnostics(syntax_diagnostics, &path, &fixed_contents);

                    panic!(
                        r#"Fixed source has a syntax error where the source document does not. This is a bug in one of the generated fixes:
{syntax_errors}
Last generated fixes:
{fixes}
Source with applied fixes:
{fixed_contents}"#
                    );
                }
            }

            diagnostics = fixed_diagnostics;
            contents = fixed_contents.to_string();
        }
    }

    let source_code = SourceFileBuilder::new(
        path.file_name().unwrap().to_string_lossy().as_ref(),
        contents,
    )
    .finish();

    Ok(diagnostics
        .into_iter()
        .map(|diagnostic| {
            // Not strictly necessary but adds some coverage for this code path
            let noqa = directives.noqa_line_for.resolve(diagnostic.start());

            Message::from_diagnostic(diagnostic, source_code.clone(), noqa)
        })
        .sorted()
        .collect())
}

fn print_diagnostics(diagnostics: Vec<Diagnostic>, file_path: &Path, source: &str) -> String {
    let source_file = SourceFileBuilder::new(
        file_path.file_name().unwrap().to_string_lossy().as_ref(),
        source,
    )
    .finish();

    let messages: Vec<_> = diagnostics
        .into_iter()
        .map(|diagnostic| {
            let noqa_start = diagnostic.start();

            Message::from_diagnostic(diagnostic, source_file.clone(), noqa_start)
        })
        .collect();

    print_messages(&messages)
}

pub(crate) fn print_messages(messages: &[Message]) -> String {
    let mut output = Vec::new();

    TextEmitter::default()
        .with_show_fix_status(true)
        .with_show_fix(true)
        .with_show_source(true)
        .emit(
            &mut output,
            messages,
            &EmitterContext::new(&FxHashMap::default()),
        )
        .unwrap();

    String::from_utf8(output).unwrap()
}

#[macro_export]
macro_rules! assert_messages {
    ($value:expr, @$snapshot:literal) => {{
        insta::with_settings!({ omit_expression => true }, {
            insta::assert_snapshot!($crate::test::print_messages(&$value), $snapshot);
        });
    }};
    ($name:expr, $value:expr) => {{
        insta::with_settings!({ omit_expression => true }, {
            insta::assert_snapshot!($name, $crate::test::print_messages(&$value));
        });
    }};
    ($value:expr) => {{
        insta::with_settings!({ omit_expression => true }, {
            insta::assert_snapshot!($crate::test::print_messages(&$value));
        });
    }};
}
