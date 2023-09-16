use anyhow::Result;
use itertools::Itertools;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::importer::ImportRequest;
use crate::{checkers::ast::Checker, registry::Rule};

/// ## What it does
/// Checks for the use of `sum()` to flatten lists of lists, which has
/// quadratic complexity.
///
/// ## Why is this bad?
/// The use of `sum()` to flatten lists of lists is quadratic in the number of
/// lists, as `sum()` creates a new list for each element in the summation.
///
/// Instead, consider using another method of flattening lists to avoid
/// quadratic complexity. The following methods are all linear in the number of
/// lists:
///
/// - `functools.reduce(operator.iconcat, lists, [])`
/// - `list(itertools.chain.from_iterable(lists)`
/// - `[item for sublist in lists for item in sublist]`
///
/// ## Example
/// ```python
/// lists = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
/// joined = sum(lists, [])
/// ```
///
/// Use instead:
/// ```python
/// import functools
/// import operator
///
///
/// lists = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
/// functools.reduce(operator.iconcat, lists, [])
/// ```
///
/// ## References
/// - [_How Not to Flatten a List of Lists in Python_](https://mathieularose.com/how-not-to-flatten-a-list-of-lists-in-python)
/// - [_How do I make a flat list out of a list of lists?_](https://stackoverflow.com/questions/952914/how-do-i-make-a-flat-list-out-of-a-list-of-lists/953097#953097)
#[violation]
pub struct QuadraticListSummation;

impl AlwaysAutofixableViolation for QuadraticListSummation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid quadratic list summation")
    }

    fn autofix_title(&self) -> String {
        format!("Replace with `functools.reduce`")
    }
}

/// RUF017
pub(crate) fn quadratic_list_summation(checker: &mut Checker, call: &ast::ExprCall) {
    let ast::ExprCall {
        func,
        arguments,
        range,
    } = call;

    if !func_is_builtin(func, "sum", checker.semantic()) {
        return;
    }

    if !start_is_empty_list(arguments, checker.semantic()) {
        return;
    };

    let Some(iterable) = arguments.args.first() else {
        return;
    };

    let mut diagnostic = Diagnostic::new(QuadraticListSummation, *range);
    if checker.patch(Rule::QuadraticListSummation) {
        diagnostic.try_set_fix(|| convert_to_reduce(iterable, call, checker));
    }
    checker.diagnostics.push(diagnostic);
}

/// Generate a [`Fix`] to convert a `sum()` call to a `functools.reduce()` call.
fn convert_to_reduce(iterable: &Expr, call: &ast::ExprCall, checker: &Checker) -> Result<Fix> {
    let (reduce_edit, reduce_binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("functools", "reduce"),
        call.start(),
        checker.semantic(),
    )?;

    let (iadd_edit, iadd_binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("operator", "iadd"),
        iterable.start(),
        checker.semantic(),
    )?;

    let iterable = checker.locator().slice(iterable);

    Ok(Fix::suggested_edits(
        Edit::range_replacement(
            format!("{reduce_binding}({iadd_binding}, {iterable}, [])"),
            call.range(),
        ),
        [reduce_edit, iadd_edit].into_iter().dedup(),
    ))
}

/// Check if a function is a builtin with a given name.
fn func_is_builtin(func: &Expr, name: &str, semantic: &SemanticModel) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return false;
    };
    id == name && semantic.is_builtin(id)
}

/// Returns `true` if the `start` argument to a `sum()` call is an empty list.
fn start_is_empty_list(arguments: &Arguments, semantic: &SemanticModel) -> bool {
    let Some(start_arg) = arguments.find_argument("start", 1) else {
        return false;
    };

    match start_arg {
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => arguments.is_empty() && func_is_builtin(func, "list", semantic),
        Expr::List(ast::ExprList { elts, ctx, .. }) => elts.is_empty() && ctx.is_load(),
        _ => false,
    }
}
