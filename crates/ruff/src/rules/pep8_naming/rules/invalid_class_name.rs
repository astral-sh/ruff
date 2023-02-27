use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for class names that do not follow the `CamelCase` convention.
    ///
    /// ## Why is this bad?
    /// [PEP 8] recommends the use of the `CapWords` (or `CamelCase`) convention
    /// for class names:
    ///
    /// > Class names should normally use the `CapWords` convention.
    /// >
    /// > The naming convention for functions may be used instead in cases where the interface is
    /// > documented and used primarily as a callable.
    /// >
    /// > Note that there is a separate convention for builtin names: most builtin names are single
    /// > words (or two words run together), with the `CapWords` convention used only for exception
    /// > names and builtin constants.
    ///
    /// ## Example
    /// ```python
    /// class my_class:
    ///     pass
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// class MyClass:
    ///     pass
    /// ```
    ///
    /// [PEP 8]: https://peps.python.org/pep-0008/#class-names
    pub struct InvalidClassName {
        pub name: String,
    }
);
impl Violation for InvalidClassName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidClassName { name } = self;
        format!("Class name `{name}` should use CapWords convention ")
    }
}

/// N801
pub fn invalid_class_name(class_def: &Stmt, name: &str, locator: &Locator) -> Option<Diagnostic> {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    if !stripped.chars().next().map_or(false, char::is_uppercase) || stripped.contains('_') {
        return Some(Diagnostic::new(
            InvalidClassName {
                name: name.to_string(),
            },
            identifier_range(class_def, locator),
        ));
    }
    None
}
