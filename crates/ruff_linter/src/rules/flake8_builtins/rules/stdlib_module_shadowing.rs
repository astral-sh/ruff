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
/// Checks for modules that use the same names as Python standard-library
/// modules.
///
/// ## Why is this bad?
/// Reusing a standard-library module name for the name of a module increases
/// the difficulty of reading and maintaining the code, and can cause
/// non-obvious errors. Readers may mistake the first-party module for the
/// standard-library module and vice versa.
///
/// Standard-library modules can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.builtins-allowed-modules`] configuration option.
///
/// This rule is not applied to stub files, as the name of a stub module is out
/// of the control of the author of the stub file. Instead, a stub should aim to
/// faithfully emulate the runtime module it is stubbing.
///
/// As of Python 3.13, errors from modules that use the same name as
/// standard-library modules now display a custom message.
///
/// ## Example
///
/// ```console
/// $ touch random.py
/// $ python3 -c 'from random import choice'
/// Traceback (most recent call last):
///   File "<string>", line 1, in <module>
///     from random import choice
/// ImportError: cannot import name 'choice' from 'random' (consider renaming '/random.py' since it has the same name as the standard library module named 'random' and prevents importing that standard library module)
/// ```
///
/// ## Options
/// - `lint.flake8-builtins.builtins-allowed-modules`
#[derive(ViolationMetadata)]
pub(crate) struct StdlibModuleShadowing {
    name: String,
}

impl Violation for StdlibModuleShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StdlibModuleShadowing { name } = self;
        format!("Module `{name}` shadows a Python standard-library module")
    }
}

/// A005
pub(crate) fn stdlib_module_shadowing(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    allowed_modules: &[String],
    target_version: PythonVersion,
) -> Option<Diagnostic> {
    if !PySourceType::try_from_path(path).is_some_and(PySourceType::is_py_file) {
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
        StdlibModuleShadowing {
            name: module_name.to_string(),
        },
        TextRange::default(),
    ))
}
