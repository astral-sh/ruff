#![cfg(any(test, fuzzing))]
//! Helper functions for the tests of rule implementations.

use std::path::Path;

#[cfg(not(fuzzing))]
use anyhow::Result;
use itertools::Itertools;
use ruff_textwrap::dedent;
use rustc_hash::FxHashMap;
use rustpython_parser::lexer::LexResult;

use ruff_diagnostics::{AutofixKind, Diagnostic};
use ruff_python_ast::source_code::{Indexer, Locator, SourceFileBuilder, Stylist};

use crate::autofix::{fix_file, FixResult};
use crate::directives;
#[cfg(not(fuzzing))]
use crate::jupyter::Notebook;
use crate::linter::{check_path, LinterResult};
use crate::message::{Emitter, EmitterContext, Message, TextEmitter};
use crate::packaging::detect_package_root;
use crate::registry::AsRule;
use crate::rules::pycodestyle::rules::syntax_error;
use crate::settings::{flags, Settings};
use crate::source_kind::SourceKind;

#[cfg(not(fuzzing))]
pub(crate) fn read_jupyter_notebook(path: &Path) -> Result<Notebook> {
    let path = test_resource_path("fixtures/jupyter").join(path);
    Notebook::read(&path).map_err(|err| {
        anyhow::anyhow!(
            "Failed to read notebook file `{}`: {:?}",
            path.display(),
            err
        )
    })
}

#[cfg(not(fuzzing))]
pub(crate) fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
    Path::new("./resources/test/").join(path)
}

/// Run [`check_path`] on a file in the `resources/test/fixtures` directory.
#[cfg(not(fuzzing))]
pub(crate) fn test_path(path: impl AsRef<Path>, settings: &Settings) -> Result<Vec<Message>> {
    let path = test_resource_path("fixtures").join(path);
    let contents = std::fs::read_to_string(&path)?;
    Ok(test_contents(
        &mut SourceKind::Python(contents),
        &path,
        settings,
    ))
}

#[cfg(not(fuzzing))]
pub(crate) fn test_notebook_path(
    path: impl AsRef<Path>,
    expected: impl AsRef<Path>,
    settings: &Settings,
) -> Result<(Vec<Message>, SourceKind)> {
    let mut source_kind = SourceKind::Jupyter(read_jupyter_notebook(path.as_ref())?);
    let messages = test_contents(&mut source_kind, path.as_ref(), settings);
    let expected_notebook = read_jupyter_notebook(expected.as_ref())?;
    if let SourceKind::Jupyter(notebook) = &source_kind {
        assert_eq!(notebook.cell_offsets(), expected_notebook.cell_offsets());
        assert_eq!(notebook.index(), expected_notebook.index());
        assert_eq!(notebook.content(), expected_notebook.content());
    };
    Ok((messages, source_kind))
}

/// Run [`check_path`] on a snippet of Python code.
pub fn test_snippet(contents: &str, settings: &Settings) -> Vec<Message> {
    let path = Path::new("<filename>");
    let contents = dedent(contents);
    test_contents(
        &mut SourceKind::Python(contents.to_string()),
        path,
        settings,
    )
}

thread_local! {
    static MAX_ITERATIONS: std::cell::Cell<usize> = std::cell::Cell::new(20);
}

pub fn set_max_iterations(max: usize) {
    MAX_ITERATIONS.with(|iterations| iterations.set(max));
}

pub(crate) fn max_iterations() -> usize {
    MAX_ITERATIONS.with(std::cell::Cell::get)
}

/// A convenient wrapper around [`check_path`], that additionally
/// asserts that autofixes converge after a fixed number of iterations.
fn test_contents(source_kind: &mut SourceKind, path: &Path, settings: &Settings) -> Vec<Message> {
    let contents = source_kind.content().to_string();
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
        path,
        path.parent()
            .and_then(|parent| detect_package_root(parent, &settings.namespace_packages)),
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        settings,
        flags::Noqa::Enabled,
        Some(source_kind),
    );

    let source_has_errors = error.is_some();

    // Detect autofixes that don't converge after multiple iterations.
    let mut iterations = 0;

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.fix.is_some())
    {
        let mut diagnostics = diagnostics.clone();
        let mut contents = contents.to_string();

        while let Some(FixResult {
            code: fixed_contents,
            source_map,
            ..
        }) = fix_file(&diagnostics, &Locator::new(&contents))
        {
            if iterations < max_iterations() {
                iterations += 1;
            } else {
                let output = print_diagnostics(diagnostics, path, &contents, source_kind);

                panic!(
                    "Failed to converge after {} iterations. This likely \
                     indicates a bug in the implementation of the fix. Last diagnostics:\n{}",
                    max_iterations(),
                    output
                );
            }

            if let Some(notebook) = source_kind.as_mut_jupyter() {
                notebook.update(&source_map, &fixed_contents);
            };

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
                path,
                None,
                tokens,
                &locator,
                &stylist,
                &indexer,
                &directives,
                settings,
                flags::Noqa::Enabled,
                Some(source_kind),
            );

            if let Some(fixed_error) = fixed_error {
                if !source_has_errors {
                    // Previous fix introduced a syntax error, abort
                    let fixes = print_diagnostics(diagnostics, path, &contents, source_kind);

                    let mut syntax_diagnostics = Vec::new();
                    syntax_error(&mut syntax_diagnostics, &fixed_error, &locator);
                    let syntax_errors =
                        print_diagnostics(syntax_diagnostics, path, &fixed_contents, source_kind);

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
        contents.as_str(),
    )
    .finish();

    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let rule = diagnostic.kind.rule();
            let fixable = diagnostic.fix.is_some();

            match (fixable, rule.autofixable()) {
                (true, AutofixKind::Sometimes | AutofixKind::Always)
                | (false, AutofixKind::None | AutofixKind::Sometimes) => {
                    // Ok
                }
                (true, AutofixKind::None) => {
                    panic!("Rule {rule:?} is marked as non-fixable but it created a fix. Change the `Violation::AUTOFIX` to either `AutofixKind::Sometimes` or `AutofixKind::Always`");
                },
                (false, AutofixKind::Always) => {
                    panic!("Rule {rule:?} is marked to always-fixable but the diagnostic has no fix. Either ensure you always emit a fix or change `Violation::AUTOFIX` to either `AutofixKind::Sometimes` or `AutofixKind::None")
                }
            }

            assert!(!(fixable && diagnostic.kind.suggestion.is_none()), "Diagnostic emitted by {rule:?} is fixable but `Violation::autofix_title` returns `None`.`");

            // Not strictly necessary but adds some coverage for this code path
            let noqa = directives.noqa_line_for.resolve(diagnostic.start());

            Message::from_diagnostic(diagnostic, source_code.clone(), noqa)
        })
        .sorted()
        .collect()
}

fn print_diagnostics(
    diagnostics: Vec<Diagnostic>,
    file_path: &Path,
    source: &str,
    source_kind: &SourceKind,
) -> String {
    let filename = file_path.file_name().unwrap().to_string_lossy();
    let source_file = SourceFileBuilder::new(filename.as_ref(), source).finish();

    let messages: Vec<_> = diagnostics
        .into_iter()
        .map(|diagnostic| {
            let noqa_start = diagnostic.start();

            Message::from_diagnostic(diagnostic, source_file.clone(), noqa_start)
        })
        .collect();

    if source_kind.is_jupyter() {
        print_jupyter_messages(&messages, &filename, source_kind)
    } else {
        print_messages(&messages)
    }
}

pub(crate) fn print_jupyter_messages(
    messages: &[Message],
    filename: &str,
    source_kind: &SourceKind,
) -> String {
    let mut output = Vec::new();

    TextEmitter::default()
        .with_show_fix_status(true)
        .with_show_fix_diff(true)
        .with_show_source(true)
        .emit(
            &mut output,
            messages,
            &EmitterContext::new(&FxHashMap::from_iter([(
                filename.to_string(),
                source_kind.clone(),
            )])),
        )
        .unwrap();

    String::from_utf8(output).unwrap()
}

pub(crate) fn print_messages(messages: &[Message]) -> String {
    let mut output = Vec::new();

    TextEmitter::default()
        .with_show_fix_status(true)
        .with_show_fix_diff(true)
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
    ($value:expr, $path:expr, $source_kind:expr) => {{
        insta::with_settings!({ omit_expression => true }, {
            insta::assert_snapshot!(
                $crate::test::print_jupyter_messages(&$value, &$path, &$source_kind)
            );
        });
    }};
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
