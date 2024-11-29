use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::PySourceType;
use ruff_python_stdlib::path::is_module_file;
use ruff_python_stdlib::sys::is_known_standard_library;
use ruff_text_size::TextRange;

use crate::package::PackageRoot;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for modules that use the same names as Python builtin modules.
///
/// ## Why is this bad?
/// Reusing a builtin module name for the name of a module increases the
/// difficulty of reading and maintaining the code, and can cause
/// non-obvious errors, as readers may mistake the variable for the
/// builtin and vice versa.
///
/// Builtin modules can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.builtins-allowed-modules`] configuration option.
///
/// ## Options
/// - `lint.flake8-builtins.builtins-allowed-modules`
#[derive(ViolationMetadata)]
pub(crate) struct BuiltinModuleShadowing {
    name: String,
}

impl Violation for BuiltinModuleShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinModuleShadowing { name } = self;
        format!("Module `{name}` is shadowing a Python builtin module")
    }
}

/// A005
pub(crate) fn builtin_module_shadowing(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    allowed_modules: &[String],
    target_version: PythonVersion,
) -> Option<Diagnostic> {
    if !PySourceType::try_from_path(path).is_some_and(PySourceType::is_py_file_or_stub) {
        return None;
    }

    let package = package?;

    let module_name = if is_module_file(path) {
        package.path().file_name().unwrap().to_string_lossy()
    } else {
        path.file_stem().unwrap().to_string_lossy()
    };

    if !is_known_standard_library(target_version.minor(), &module_name) {
        return None;
    }

    // Shadowing private stdlib modules is okay.
    // https://github.com/astral-sh/ruff/issues/12949
    if module_name.starts_with('_') && !module_name.starts_with("__") {
        return None;
    }

    if allowed_modules
        .iter()
        .any(|allowed_module| allowed_module == &module_name)
    {
        return None;
    }

    Some(Diagnostic::new(
        BuiltinModuleShadowing {
            name: module_name.to_string(),
        },
        TextRange::default(),
    ))
}
