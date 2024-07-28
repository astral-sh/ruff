use std::ffi::OsStr;
use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::identifiers::{is_migration_name, is_module_name};
use ruff_python_stdlib::path::is_module_file;
use ruff_text_size::TextRange;

use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for module names that do not follow the `snake_case` naming
/// convention or are otherwise invalid.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of the `snake_case` naming convention for
/// module names:
///
/// > Modules should have short, all-lowercase names. Underscores can be used in the
/// > module name if it improves readability. Python packages should also have short,
/// > all-lowercase names, although the use of underscores is discouraged.
/// >
/// > When an extension module written in C or C++ has an accompanying Python module that
/// > provides a higher level (e.g. more object-oriented) interface, the C/C++ module has
/// > a leading underscore (e.g. `_socket`).
///
/// Further, in order for Python modules to be importable, they must be valid
/// identifiers. As such, they cannot start with a digit, or collide with hard
/// keywords, like `import` or `class`.
///
/// ## Example
/// - Instead of `example-module-name` or `example module name`, use `example_module_name`.
/// - Instead of `ExampleModule`, use `example_module`.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#package-and-module-names
#[violation]
pub struct InvalidModuleName {
    name: String,
}

impl Violation for InvalidModuleName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidModuleName { name } = self;
        format!("Invalid module name: '{name}'")
    }
}

/// N999
pub(crate) fn invalid_module_name(
    path: &Path,
    package: Option<&Path>,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if !path
        .extension()
        .is_some_and(|ext| ext == "py" || ext == "pyi")
    {
        return None;
    }

    if let Some(package) = package {
        let module_name = if is_module_file(path) {
            package.file_name().unwrap().to_string_lossy()
        } else {
            path.file_stem().unwrap().to_string_lossy()
        };

        // As a special case, we allow files in `versions` and `migrations` directories to start
        // with a digit (e.g., `0001_initial.py`), to support common conventions used by Django
        // and other frameworks.
        let is_valid_module_name = if is_migration_file(path) {
            is_migration_name(&module_name)
        } else {
            is_module_name(&module_name)
        };

        if !is_valid_module_name {
            // Ignore any explicitly-allowed names.
            if ignore_names.matches(&module_name) {
                return None;
            }
            return Some(Diagnostic::new(
                InvalidModuleName {
                    name: module_name.to_string(),
                },
                TextRange::default(),
            ));
        }
    }

    None
}

/// Return `true` if a [`Path`] refers to a migration file.
fn is_migration_file(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .is_some_and(|parent| matches!(parent, "versions" | "migrations"))
}
