use anyhow::Result;
use itertools::Itertools;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::AstNode;
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
/// - `functools.reduce(operator.iadd, lists, [])`
/// - `list(itertools.chain.from_iterable(lists))`
/// - `[item for sublist in lists for item in sublist]`
///
/// When fixing relevant violations, Ruff defaults to the `functools.reduce`
/// form, which outperforms the other methods in [microbenchmarks].
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
/// functools.reduce(operator.iadd, lists, [])
/// ```
///
/// ## References
/// - [_How Not to Flatten a List of Lists in Python_](https://mathieularose.com/how-not-to-flatten-a-list-of-lists-in-python)
/// - [_How do I make a flat list out of a list of lists?_](https://stackoverflow.com/questions/952914/how-do-i-make-a-flat-list-out-of-a-list-of-lists/953097#953097)
///
/// [microbenchmarks]: https://github.com/astral-sh/ruff/issues/5073#issuecomment-1591836349
#[violation]
pub struct QuadraticListSummation;

impl AlwaysFixableViolation for QuadraticListSummation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid quadratic list summation")
    }

    fn fix_title(&self) -> String {
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

    let Some(iterable) = arguments.args.first() else {
        return;
    };

    let semantic = checker.semantic();

    if !semantic.match_builtin_expr(func, "sum") {
        return;
    }

    if !start_is_empty_list(arguments, semantic) {
        return;
    };

    let mut diagnostic = Diagnostic::new(QuadraticListSummation, *range);
    diagnostic.try_set_fix(|| convert_to_reduce(iterable, call, checker));
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

    let iterable = checker.locator().slice(
        parenthesized_range(
            iterable.into(),
            call.arguments.as_any_node_ref(),
            checker.comment_ranges(),
            checker.locator().contents(),
        )
        .unwrap_or(iterable.range()),
    );

    Ok(Fix::unsafe_edits(
        Edit::range_replacement(
            format!("{reduce_binding}({iadd_binding}, {iterable}, [])"),
            call.range(),
        ),
        [reduce_edit, iadd_edit].into_iter().dedup(),
    ))
}

/// Returns `true` if the `start` argument to a `sum()` call is an empty list.
fn start_is_empty_list(arguments: &Arguments, semantic: &SemanticModel) -> bool {
    let Some(start_arg) = arguments.find_argument("start", 1) else {
        return false;
    };

    match start_arg {
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => arguments.is_empty() && semantic.match_builtin_expr(func, "list"),
        Expr::List(list) => list.is_empty() && list.ctx.is_load(),
        _ => false,
    }
}
