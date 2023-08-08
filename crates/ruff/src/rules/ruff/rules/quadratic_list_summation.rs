use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_text_size::TextRange;

use crate::{checkers::ast::Checker, registry::Rule};

/// ## What it does
/// Avoid quadratic
///
/// ## Why is this bad?
/// Quadratic list summation is slower than other methods of list concatenation.
/// A list comprehension can perform the same operation but in a much shorter time period.
///
/// ## Example
/// ```python
/// lists = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
/// joined = sum(lists, [])
/// ```
///
/// Use instead:
/// ```python
/// lists = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
/// joined = [el for list in lists for el in list]
/// ```
#[violation]
pub struct QuadraticListSummation;

impl AlwaysAutofixableViolation for QuadraticListSummation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace quadratic list summation with list comprehension")
    }

    fn autofix_title(&self) -> String {
        format!("Convert to list comprehension")
    }
}

/// RUF017
pub(crate) fn quadratic_list_summation(checker: &mut Checker, call: &ast::ExprCall) {
    let ast::ExprCall {
        func,
        arguments,
        range,
    } = call;

    if !func_is_builtin(func, "sum", checker) {
        return;
    }

    if verify_start_arg(arguments, checker).is_none() {
        return;
    }

    let Arguments { args, .. } = arguments;
    let Some(list_string_repr) = args
        .first()
        .and_then(|arg| match arg {
            Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str()),
            Expr::List(ast::ExprList { range, .. }) => Some(checker.locator().slice(*range)),
            _ => None,
        }) else {
            return;
        };

    let mut diagnostic = Diagnostic::new(QuadraticListSummation, *range);
    if checker.patch(Rule::QuadraticListSummation) {
        diagnostic.set_fix(convert_to_comprehension(list_string_repr, *range));
    }

    checker.diagnostics.push(diagnostic);
}

fn convert_to_comprehension(container_list_name: &str, range: TextRange) -> Fix {
    Fix::suggested(Edit::range_replacement(
        format!("[el for lst in {container_list_name} for el in lst]"),
        range,
    ))
}

/// Check that the `start` arg/kwarg is actually a list/list-equivalent.
fn verify_start_arg<'a>(arguments: &'a Arguments, checker: &mut Checker) -> Option<&'a Expr> {
    let Some(start_arg) = arguments.find_argument("start", 1) else {
        return None;
    };

    match start_arg {
        Expr::Call(ast::ExprCall { func, .. }) => {
            if func_is_builtin(func, "list", checker) {
                Some(start_arg)
            } else {
                None
            }
        }
        Expr::List(ast::ExprList { elts, ctx, .. }) => {
            if elts.is_empty() && ctx == &ast::ExprContext::Load {
                Some(start_arg)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if a function is builtin with a given name.
fn func_is_builtin(func: &Expr, name: &str, checker: &mut Checker) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return false;
    };

    id == name && checker.semantic().is_builtin(id)
}
