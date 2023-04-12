#![cfg(test)]

/// Helper functions for the tests of rule implementations.
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use rustpython_parser::lexer::LexResult;

use ruff_python_ast::source_code::{Indexer, Locator, SourceFileBuilder, Stylist};

use crate::autofix::fix_file;
use crate::directives;
use crate::linter::{check_path, LinterResult};
use crate::message::{Emitter, EmitterContext, Message, TextEmitter};
use crate::packaging::detect_package_root;
use crate::settings::{flags, Settings};

pub fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
    Path::new("./resources/test/").join(path)
}

/// A convenient wrapper around [`check_path`], that additionally
/// asserts that autofixes converge after 10 iterations.
pub fn test_path(path: impl AsRef<Path>, settings: &Settings) -> Result<Vec<Message>> {
    let path = test_resource_path("fixtures").join(path);
    let contents = std::fs::read_to_string(&path)?;
    let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&contents);
    let locator = Locator::new(&contents);
    let stylist = Stylist::from_tokens(&tokens, &locator);
    let indexer: Indexer = tokens.as_slice().into();
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
    let LinterResult {
        data: (diagnostics, _imports),
        ..
    } = check_path(
        &path,
        path.parent()
            .and_then(|parent| detect_package_root(parent, &settings.namespace_packages)),
        &contents,
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        settings,
        flags::Noqa::Enabled,
        flags::Autofix::Enabled,
    );

    // Detect autofixes that don't converge after multiple iterations.
    if diagnostics
        .iter()
        .any(|diagnostic| !diagnostic.fix.is_empty())
    {
        let max_iterations = 10;

        let mut contents = contents.clone();
        let mut iterations = 0;

        loop {
            let tokens: Vec<LexResult> = ruff_rustpython::tokenize(&contents);
            let locator = Locator::new(&contents);
            let stylist = Stylist::from_tokens(&tokens, &locator);
            let indexer: Indexer = tokens.as_slice().into();
            let directives =
                directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
            let LinterResult {
                data: (diagnostics, _imports),
                ..
            } = check_path(
                &path,
                None,
                &contents,
                tokens,
                &locator,
                &stylist,
                &indexer,
                &directives,
                settings,
                flags::Noqa::Enabled,
                flags::Autofix::Enabled,
            );
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

    let source_code = SourceFileBuilder::new(&path.file_name().unwrap().to_string_lossy())
        .source_text_string(contents)
        .finish();

    Ok(diagnostics
        .into_iter()
        .map(|diagnostic| Message::from_diagnostic(diagnostic, source_code.clone(), 1))
        .sorted()
        .collect())
}

pub(crate) fn print_messages(messages: &[Message]) -> String {
    let mut output = Vec::new();

    TextEmitter::default()
        .with_show_fix_status(true)
        .with_show_fix(true)
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
