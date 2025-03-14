use ruff_python_ast::{Arguments, Expr, ExprCall};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary `list()` calls around list comprehensions.
///
/// ## Why is this bad?
/// It is redundant to use a `list()` call around a list comprehension.
///
/// ## Example
/// ```python
/// list([f(x) for x in foo])
/// ```
///
/// Use instead
/// ```python
/// [f(x) for x in foo]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryListCall;

impl AlwaysFixableViolation for UnnecessaryListCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary `list()` call (remove the outer call to `list()`)".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove outer `list()` call".to_string()
    }
}

/// C411
pub(crate) fn unnecessary_list_call(checker: &Checker, expr: &Expr, call: &ExprCall) {
    let ExprCall {
        func,
        arguments,
        range: _,
    } = call;

    if !arguments.keywords.is_empty() {
        return;
    }

    if arguments.args.len() > 1 {
        return;
    }

    let Arguments {
        range: _,
        args,
        keywords: _,
    } = arguments;

    let Some(argument) = helpers::first_argument_with_matching_function("list", func, args) else {
        return;
    };
    if !argument.is_list_comp_expr() {
        return;
    }
    if !checker.semantic().has_builtin_binding("list") {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryListCall, expr.range());
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_list_call(expr, checker.locator(), checker.stylist())
            .map(Fix::unsafe_edit)
    });
    checker.report_diagnostic(diagnostic);
}
