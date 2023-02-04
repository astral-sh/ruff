use std::path::Path;

use crate::registry::{Diagnostic, Rule};
use crate::rules::flake8_no_pep420::rules::implicit_namespace_package;
use crate::settings::Settings;

pub fn check_file_path(
    path: &Path,
    package: Option<&Path>,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // flake8-no-pep420
    if settings.rules.enabled(&Rule::ImplicitNamespacePackage) {
        if let Some(diagnostic) =
            implicit_namespace_package(path, package, &settings.project_root, &settings.src)
        {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
