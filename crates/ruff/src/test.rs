#![cfg(any(test, fuzzing))]
//! Helper functions for the tests of rule implementations.

use std::borrow::Cow;
use std::path::Path;

#[cfg(not(fuzzing))]
use anyhow::Result;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AutofixKind, Diagnostic};
use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::AsMode;
use ruff_python_trivia::textwrap::dedent;
use ruff_source_file::{Locator, SourceFileBuilder};
use ruff_text_size::Ranged;

use crate::autofix::{fix_file, FixResult};
use crate::directives;
use crate::linter::{check_path, LinterResult};
use crate::message::{Emitter, EmitterContext, Message, TextEmitter};
use crate::packaging::detect_package_root;
use crate::registry::AsRule;
use crate::rules::pycodestyle::rules::syntax_error;
use crate::settings::{flags, Settings};
use crate::source_kind::SourceKind;
use ruff_notebook::{Notebook, NotebookError};

#[cfg(not(fuzzing))]
pub(crate) fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
    Path::new("./resources/test/").join(path)
}

/// Run [`check_path`] on a file in the `resources/test/fixtures` directory.
#[cfg(not(fuzzing))]
pub(crate) fn test_path(path: impl AsRef<Path>, settings: &Settings) -> Result<Vec<Message>> {
    let path = test_resource_path("fixtures").join(path);
    let contents = std::fs::read_to_string(&path)?;
    Ok(test_contents(&SourceKind::Python(contents), &path, settings).0)
}

#[cfg(not(fuzzing))]
pub(crate) struct TestedNotebook {
    pub(crate) messages: Vec<Message>,
    pub(crate) source_notebook: Notebook,
    pub(crate) linted_notebook: Notebook,
}

#[cfg(not(fuzzing))]
pub(crate) fn test_notebook_path(
    path: impl AsRef<Path>,
    expected: impl AsRef<Path>,
    settings: &Settings,
) -> Result<TestedNotebook, NotebookError> {
    let source_notebook = Notebook::from_path(path.as_ref())?;

    let source_kind = SourceKind::IpyNotebook(source_notebook);
    let (messages, transformed) = test_contents(&source_kind, path.as_ref(), settings);
    let expected_notebook = Notebook::from_path(expected.as_ref())?;
    let linted_notebook = transformed.into_owned().expect_ipy_notebook();

    assert_eq!(
        linted_notebook.cell_offsets(),
        expected_notebook.cell_offsets()
    );
    assert_eq!(linted_notebook.index(), expected_notebook.index());
    assert_eq!(
        linted_notebook.source_code(),
        expected_notebook.source_code()
    );

    Ok(TestedNotebook {
        messages,
        source_notebook: source_kind.expect_ipy_notebook(),
        linted_notebook,
    })
}

/// Run [`check_path`] on a snippet of Python code.
pub fn test_snippet(contents: &str, settings: &Settings) -> Vec<Message> {
    let path = Path::new("<filename>");
    let contents = dedent(contents);
    test_contents(&SourceKind::Python(contents.into_owned()), path, settings).0
}

thread_local! {
    static MAX_ITERATIONS: std::cell::Cell<usize> = std::cell::Cell::new(8);
}

pub fn set_max_iterations(max: usize) {
    MAX_ITERATIONS.with(|iterations| iterations.set(max));
}

pub(crate) fn max_iterations() -> usize {
    MAX_ITERATIONS.with(std::cell::Cell::get)
}

/// A convenient wrapper around [`check_path`], that additionally
/// asserts that autofixes converge after a fixed number of iterations.
pub(crate) fn test_contents<'a>(
    source_kind: &'a SourceKind,
    path: &Path,
    settings: &Settings,
) -> (Vec<Message>, Cow<'a, SourceKind>) {
    let source_type = PySourceType::from(path);
    let tokens: Vec<LexResult> =
        ruff_python_parser::tokenize(source_kind.source_code(), source_type.as_mode());
    let locator = Locator::new(source_kind.source_code());
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
        source_kind,
        source_type,
    );

    let source_has_errors = error.is_some();

    // Detect autofixes that don't converge after multiple iterations.
    let mut iterations = 0;

    let mut transformed = Cow::Borrowed(source_kind);

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.fix.is_some())
    {
        let mut diagnostics = diagnostics.clone();

        while let Some(FixResult {
            code: fixed_contents,
            source_map,
            ..
        }) = fix_file(&diagnostics, &Locator::new(transformed.source_code()))
        {
            if iterations < max_iterations() {
                iterations += 1;
            } else {
                let output = print_diagnostics(diagnostics, path, &transformed);

                panic!(
                    "Failed to converge after {} iterations. This likely \
                     indicates a bug in the implementation of the fix. Last diagnostics:\n{}",
                    max_iterations(),
                    output
                );
            }

            transformed = Cow::Owned(transformed.updated(fixed_contents, &source_map));

            let tokens: Vec<LexResult> =
                ruff_python_parser::tokenize(transformed.source_code(), source_type.as_mode());
            let locator = Locator::new(transformed.source_code());
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
                source_kind,
                source_type,
            );

            if let Some(fixed_error) = fixed_error {
                if !source_has_errors {
                    // Previous fix introduced a syntax error, abort
                    let fixes = print_diagnostics(diagnostics, path, source_kind);

                    let mut syntax_diagnostics = Vec::new();
                    syntax_error(&mut syntax_diagnostics, &fixed_error, &locator);
                    let syntax_errors = print_diagnostics(syntax_diagnostics, path, &transformed);

                    panic!(
                        r#"Fixed source has a syntax error where the source document does not. This is a bug in one of the generated fixes:
{syntax_errors}
Last generated fixes:
{fixes}
Source with applied fixes:
{}"#,
                        transformed.source_code()
                    );
                }
            }

            diagnostics = fixed_diagnostics;
        }
    }

    let source_code = SourceFileBuilder::new(
        path.file_name().unwrap().to_string_lossy().as_ref(),
        source_kind.source_code(),
    )
    .finish();

    let messages = diagnostics
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
        .collect();
    (messages, transformed)
}

fn print_diagnostics(diagnostics: Vec<Diagnostic>, path: &Path, source: &SourceKind) -> String {
    let filename = path.file_name().unwrap().to_string_lossy();
    let source_file = SourceFileBuilder::new(filename.as_ref(), source.source_code()).finish();

    let messages: Vec<_> = diagnostics
        .into_iter()
        .map(|diagnostic| {
            let noqa_start = diagnostic.start();

            Message::from_diagnostic(diagnostic, source_file.clone(), noqa_start)
        })
        .collect();

    if let Some(notebook) = source.as_ipy_notebook() {
        print_jupyter_messages(&messages, path, notebook)
    } else {
        print_messages(&messages)
    }
}

pub(crate) fn print_jupyter_messages(
    messages: &[Message],
    path: &Path,
    notebook: &Notebook,
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
                path.file_name().unwrap().to_string_lossy().to_string(),
                notebook.index().clone(),
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
    ($value:expr, $path:expr, $notebook:expr) => {{
        insta::with_settings!({ omit_expression => true }, {
            insta::assert_snapshot!(
                $crate::test::print_jupyter_messages(&$value, &$path, &$notebook)
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
