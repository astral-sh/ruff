use std::path::Path;

use anyhow::Result;
use path_absolutize::path_dedot;
use rustpython_parser::lexer::LexResult;

use crate::linter::check_path;
use crate::registry::Diagnostic;
use crate::resolver::Relativity;
use crate::rustpython_helpers::tokenize;
use crate::settings::configuration::Configuration;
use crate::settings::{flags, pyproject, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::{directives, packaging, resolver};

/// Load the relevant `Settings` for a given `Path`.
fn resolve(path: &Path) -> Result<Settings> {
    if let Some(pyproject) = pyproject::find_settings_toml(path)? {
        // First priority: `pyproject.toml` in the current `Path`.
        Ok(resolver::resolve_settings(&pyproject, &Relativity::Parent)?.lib)
    } else if let Some(pyproject) = pyproject::find_user_settings_toml() {
        // Second priority: user-specific `pyproject.toml`.
        Ok(resolver::resolve_settings(&pyproject, &Relativity::Cwd)?.lib)
    } else {
        // Fallback: default settings.
        Settings::from_configuration(Configuration::default(), &path_dedot::CWD)
    }
}

/// Run Ruff over Python source code directly.
pub fn check(path: &Path, contents: &str, autofix: bool) -> Result<Vec<Diagnostic>> {
    // Load the relevant `Settings` for the given `Path`.
    let settings = resolve(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_contents(contents, &locator);

    // Extra indices from the code.
    let indexer: Indexer = tokens.as_slice().into();

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives =
        directives::extract_directives(&tokens, directives::Flags::from_settings(&settings));

    // Generate diagnostics.
    let result = check_path(
        path,
        packaging::detect_package_root(path, &settings.namespace_packages),
        contents,
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        &settings,
        autofix.into(),
        flags::Noqa::Enabled,
    );

    Ok(result.data)
}
