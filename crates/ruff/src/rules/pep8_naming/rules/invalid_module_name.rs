use std::path::Path;

use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string::is_lower_with_underscore;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for module names that do not follow the [`snake_case`] naming convention.
    ///
    /// ## Why is this bad?
    /// Module names that follow the `snake_case` naming convention are recommended by [PEP8]:
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
    /// * Instead of `example-module-name` or `example module name`, use `example_module_name`
    /// * Instead of `ExampleModule`, use `example_module`,
    ///
    /// [PEP8]: https://peps.python.org/pep-0008/#package-and-module-names
    /// [`snake_case`]: https://en.wikipedia.org/wiki/Snake_case
    ///
    pub struct InvalidModuleName {
        pub name: String,
    }
);
impl Violation for InvalidModuleName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidModuleName { name } = self;
        format!("Invalid module name: '{name}'")
    }
}

/// N999
pub fn invalid_module_name(path: &Path, package: Option<&Path>) -> Option<Diagnostic> {
    if let Some(package) = package {
        let module_name = if path.file_name().unwrap().to_string_lossy() == "__init__.py" {
            package.file_name().unwrap().to_string_lossy()
        } else {
            path.file_stem().unwrap().to_string_lossy()
        };

        if !is_lower_with_underscore(&module_name) {
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
