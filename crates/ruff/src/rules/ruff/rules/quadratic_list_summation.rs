use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_text_size::TextRange;

use crate::{checkers::ast::Checker, registry::Rule};

/// ## What it does
/// Avoid quadratic list summation, or the flattening of multiple lists into one via the use of the
/// `sum()` built-in function.
///
/// ## Why is this bad?
/// Quadratic list summation is slower than other methods of list flattening. See the link in
/// [`References`](#references) for `timeit` results on other ways of flattening lists.
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
///
/// ## References
/// [How do I make a flat list out of a list of lists?](https://stackoverflow.com/questions/952914/how-do-i-make-a-flat-list-out-of-a-list-of-lists/953097#953097)
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

    if !start_is_list(arguments, checker) {
        return;
    };

    let Arguments { args, .. } = arguments;
    let Some(first) = args.first() else {
        return;
    };

    let list_as_str = match first {
        Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str()),
        Expr::List(ast::ExprList { range, .. }) => Some(checker.locator().slice(*range)),
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(QuadraticListSummation, *range);
    if checker.patch(Rule::QuadraticListSummation) {
        let Some(str_repr) = list_as_str else {
            return;
        };

        diagnostic.set_fix(convert_to_comprehension(str_repr, *range));
    }

    checker.diagnostics.push(diagnostic);
}

/// Check if a function is a builtin with a given name.
fn func_is_builtin(func: &Expr, name: &str, checker: &mut Checker) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return false;
    };

    id == name && checker.semantic().is_builtin(id)
}

fn convert_to_comprehension(container_list_name: &str, range: TextRange) -> Fix {
    Fix::suggested(Edit::range_replacement(
        format!("[el for lst in {container_list_name} for el in lst]"),
        range,
    ))
}

fn start_is_list(arguments: &Arguments, checker: &mut Checker) -> bool {
    let Some(start_arg) = arguments.find_argument("start", 1) else {
        return false;
    };

    match start_arg {
        Expr::Call(ast::ExprCall { func, .. }) => func_is_builtin(func, "list", checker),
        Expr::List(ast::ExprList { elts, ctx, .. }) => {
            elts.is_empty() && ctx == &ast::ExprContext::Load
        }
        _ => false,
    }
}
