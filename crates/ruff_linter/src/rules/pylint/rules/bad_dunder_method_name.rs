use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_known_dunder_method;

/// ## What it does
/// Checks for dunder methods that have no special meaning in Python 3.
///
/// ## Why is this bad?
/// Misspelled or no longer supported dunder name methods may cause your code to not function
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
/// [`lint.pylint.allow-dunder-method-names`] setting.
///
/// ## Example
///
/// ```python
/// class Foo:
///     def __init_(self): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo:
///     def __init__(self): ...
/// ```
///
/// ## Options
/// - `lint.pylint.allow-dunder-method-names`
#[violation]
pub struct BadDunderMethodName {
    name: String,
}

impl Violation for BadDunderMethodName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadDunderMethodName { name } = self;
        format!("Dunder method `{name}` has no special meaning in Python 3")
    }
}

/// PLW3201
pub(crate) fn bad_dunder_method_name(checker: &mut Checker, method: &ast::StmtFunctionDef) {
    // If the name isn't a dunder, skip it.
    if !method.name.starts_with('_') || !method.name.ends_with('_') {
        return;
    }

    // If the name is explicitly allowed, skip it.
    if is_known_dunder_method(&method.name)
        || checker
            .settings
            .pylint
            .allow_dunder_method_names
            .contains(method.name.as_str())
        || matches!(method.name.as_str(), "_")
    {
        return;
    }

    if visibility::is_override(&method.decorator_list, checker.semantic()) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        BadDunderMethodName {
            name: method.name.to_string(),
        },
        method.identifier(),
    ));
}
