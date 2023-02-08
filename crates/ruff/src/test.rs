/// Helper functions for the tests of rule implementations.
use std::path::Path;

use anyhow::Result;
use rustpython_parser::lexer::LexResult;

use crate::autofix::fix_file;
use crate::linter::{check_path, LinterResult};
use crate::packaging::detect_package_root;
use crate::registry::Diagnostic;
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::{directives, fs, rustpython_helpers};

pub fn test_resource_path(path: impl AsRef<Path>) -> std::path::PathBuf {
    Path::new("./resources/test/").join(path)
}

/// A convenient wrapper around [`check_path`], that additionally
/// asserts that autofixes converge after 10 iterations.
pub fn test_path(path: &Path, settings: &Settings) -> Result<Vec<Diagnostic>> {
    let path = test_resource_path("fixtures").join(path);
    let contents = fs::read_file(&path)?;
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
    let locator = Locator::new(&contents);
    let stylist = Stylist::from_contents(&contents, &locator);
    let indexer: Indexer = tokens.as_slice().into();
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(settings));
    let LinterResult {
        data: mut diagnostics,
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
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    );

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
            let LinterResult {
                data: diagnostics, ..
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
                flags::Autofix::Enabled,
                flags::Noqa::Enabled,
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

    diagnostics.sort_by_key(|diagnostic| diagnostic.location);
    Ok(diagnostics)
}
