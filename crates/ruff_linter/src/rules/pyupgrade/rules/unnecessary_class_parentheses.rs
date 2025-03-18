use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for class definitions that include unnecessary parentheses after
/// the class name.
///
/// ## Why is this bad?
/// If a class definition doesn't have any bases, the parentheses are
/// unnecessary.
///
/// ## Example
/// ```python
/// class Foo():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     ...
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryClassParentheses;

impl AlwaysFixableViolation for UnnecessaryClassParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary parentheses after class definition".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove parentheses".to_string()
    }
}

/// UP039
pub(crate) fn unnecessary_class_parentheses(checker: &Checker, class_def: &ast::StmtClassDef) {
    let Some(arguments) = class_def.arguments.as_deref() else {
        return;
    };

    if !arguments.args.is_empty() || !arguments.keywords.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryClassParentheses, arguments.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
        arguments.start(),
        arguments.end(),
    )));
    checker.report_diagnostic(diagnostic);
}
