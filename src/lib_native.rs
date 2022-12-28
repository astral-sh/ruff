use std::path::Path;

use anyhow::Result;
use path_absolutize::path_dedot;
use rustpython_parser::lexer::LexResult;

use crate::checks::Check;
use crate::linter::check_path;
use crate::resolver::Relativity;
use crate::rustpython_helpers::tokenize;
use crate::settings::configuration::Configuration;
use crate::settings::{flags, pyproject, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::{directives, packages, resolver};

/// Load the relevant `Settings` for a given `Path`.
fn resolve(path: &Path) -> Result<Settings> {
    if let Some(pyproject) = pyproject::find_settings_toml(path)? {
        // First priority: `pyproject.toml` in the current `Path`.
        resolver::resolve_settings(&pyproject, &Relativity::Parent, None)
    } else if let Some(pyproject) = pyproject::find_user_settings_toml() {
        // Second priority: user-specific `pyproject.toml`.
        resolver::resolve_settings(&pyproject, &Relativity::Cwd, None)
    } else {
        // Fallback: default settings.
        Settings::from_configuration(Configuration::default(), &path_dedot::CWD)
    }
}

/// Run Ruff over Python source code directly.
pub fn check(path: &Path, contents: &str, autofix: bool) -> Result<Vec<Check>> {
    // Load the relevant `Settings` for the given `Path`.
    let settings = resolve(path)?;

    // Validate the `Settings` and return any errors.
    settings.validate()?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = SourceCodeLocator::new(contents);

    // Detect the current code style (lazily).
    let stylist = SourceCodeStyleDetector::from_contents(contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        directives::Flags::from_settings(&settings),
    );

    // Generate checks.
    let checks = check_path(
        path,
        packages::detect_package_root(path),
        contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        &settings,
        autofix.into(),
        flags::Noqa::Enabled,
    )?;

    Ok(checks)
}
