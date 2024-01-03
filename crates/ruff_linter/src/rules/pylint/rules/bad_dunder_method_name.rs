use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::Stmt;
use ruff_python_semantic::analyze::visibility;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_known_dunder_method;

/// ## What it does
/// Checks for misspelled and unknown dunder names in method definitions.
///
/// ## Why is this bad?
/// Misspelled dunder name methods may cause your code to not function
/// as expected.
///
/// Since dunder methods are associated with customizing the behavior
/// of a class in Python, introducing a dunder method such as `__foo__`
/// that diverges from standard Python dunder methods could potentially
/// confuse someone reading the code.
///
/// This rule will detect all methods starting and ending with at least
/// one underscore (e.g., `_str_`), but ignores known dunder methods (like
/// `__init__`), as well as methods that are marked with `@override`.
///
/// Additional dunder methods names can be allowed via the
/// [`pylint.allow-dunder-method-names`] setting.
///
/// ## Example
/// ```python
/// class Foo:
///     def __init_(self):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __init__(self):
///         ...
/// ```
///
/// ## Options
/// - `pylint.allow-dunder-method-names`
#[violation]
pub struct BadDunderMethodName {
    name: String,
}

impl Violation for BadDunderMethodName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadDunderMethodName { name } = self;
        format!("Bad or misspelled dunder method name `{name}`")
    }
}

/// PLW3201
pub(crate) fn bad_dunder_method_name(checker: &mut Checker, class_body: &[Stmt]) {
    for method in class_body
        .iter()
        .filter_map(ruff_python_ast::Stmt::as_function_def_stmt)
        .filter(|method| {
            if is_known_dunder_method(&method.name)
                || checker
                    .settings
                    .pylint
                    .allow_dunder_method_names
                    .contains(method.name.as_str())
                || matches!(method.name.as_str(), "_")
            {
                return false;
            }
            method.name.starts_with('_') && method.name.ends_with('_')
        })
    {
        if visibility::is_override(&method.decorator_list, checker.semantic()) {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            BadDunderMethodName {
                name: method.name.to_string(),
            },
            method.identifier(),
        ));
    }
}
