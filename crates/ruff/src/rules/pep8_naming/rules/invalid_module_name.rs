use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_stdlib::identifiers::is_module_name;

/// ## What it does
/// Checks for module names that do not follow the `snake_case` naming
/// convention.
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
/// > provides a higher level (e.g. more object oriented) interface, the C/C++ module has
/// > a leading underscore (e.g. `_socket`).
///
/// ## Example
/// - Instead of `example-module-name` or `example module name`, use `example_module_name`.
/// - Instead of `ExampleModule`, use `example_module`.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#package-and-module-names
#[violation]
pub struct InvalidModuleName {
    pub name: String,
}

impl Violation for InvalidModuleName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidModuleName { name } = self;
        format!("Invalid module name: '{name}'")
    }
}

/// N999
pub fn invalid_module_name(path: &Path, package: Option<&Path>) -> Option<Diagnostic> {
    if !path
        .extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
    {
        return None;
    }

    if let Some(package) = package {
        let module_name = if path.file_name().map_or(false, |file_name| {
            file_name == "__init__.py"
                || file_name == "__init__.pyi"
                || file_name == "__main__.py"
                || file_name == "__main__.pyi"
        }) {
            package.file_name().unwrap().to_string_lossy()
        } else {
            path.file_stem().unwrap().to_string_lossy()
        };

        if !is_module_name(&module_name) {
            return Some(Diagnostic::new(
                InvalidModuleName {
                    name: module_name.to_string(),
                },
                Range::default(),
            ));
        }
    }

    None
}
