use anyhow::Result;
use itertools::Itertools;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, Arguments, Expr, PythonVersion};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// - `[*sublist for sublist in lists]` (Python 3.15+)
/// - `functools.reduce(operator.iadd, lists, [])`
/// - `list(itertools.chain.from_iterable(lists))`
/// - `[item for sublist in lists for item in sublist]`
///
/// When fixing relevant violations, Ruff uses the starred-list-comprehension
/// form on Python 3.15 and later. On older Python versions, Ruff falls back to
/// the `functools.reduce` form, which outperforms the other pre-3.15
/// alternatives in [microbenchmarks].
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
/// joined = [*sublist for sublist in lists]
/// ```
///
/// ## Fix safety
///
/// This fix is always marked as unsafe because the replacement may accept any
/// iterable where `sum` previously required lists. On Python 3.15 and later,
/// Ruff uses iterable unpacking within a list comprehension; on older Python
/// versions, Ruff uses `operator.iadd`. In both cases, code that previously
/// raised an error may silently succeed. Moreover, the fix could remove
/// comments from the original code.
///
/// ## References
/// - [_How Not to Flatten a List of Lists in Python_](https://mathieularose.com/how-not-to-flatten-a-list-of-lists-in-python)
/// - [_How do I make a flat list out of a list of lists?_](https://stackoverflow.com/questions/952914/how-do-i-make-a-flat-list-out-of-a-list-of-lists/953097#953097)
///
/// [microbenchmarks]: https://github.com/astral-sh/ruff/issues/5073#issuecomment-1591836349
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.285")]
pub(crate) struct QuadraticListSummation {
    fix_style: QuadraticListSummationFixStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuadraticListSummationFixStyle {
    FunctoolsReduce,
    StarredListComprehension,
}

impl QuadraticListSummationFixStyle {
    fn from_target_version(target_version: PythonVersion) -> Self {
        if target_version >= PythonVersion::PY315 {
            Self::StarredListComprehension
        } else {
            Self::FunctoolsReduce
        }
    }
}

impl AlwaysFixableViolation for QuadraticListSummation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid quadratic list summation".to_string()
    }

    fn fix_title(&self) -> String {
        match self.fix_style {
            QuadraticListSummationFixStyle::FunctoolsReduce => {
                "Replace with `functools.reduce`".to_string()
            }
            QuadraticListSummationFixStyle::StarredListComprehension => {
                "Replace with a starred list comprehension".to_string()
            }
        }
    }
}

/// RUF017
pub(crate) fn quadratic_list_summation(checker: &Checker, call: &ast::ExprCall) {
    let ast::ExprCall {
        func,
        arguments,
        range,
        node_index: _,
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
    }

    let fix_style = QuadraticListSummationFixStyle::from_target_version(checker.target_version());
    let mut diagnostic = checker.report_diagnostic(QuadraticListSummation { fix_style }, *range);
    diagnostic.try_set_fix(|| convert_to_fix(iterable, call, checker, fix_style));
}

fn convert_to_fix(
    iterable: &Expr,
    call: &ast::ExprCall,
    checker: &Checker,
    fix_style: QuadraticListSummationFixStyle,
) -> Result<Fix> {
    match fix_style {
        QuadraticListSummationFixStyle::FunctoolsReduce => {
            convert_to_reduce(iterable, call, checker)
        }
        QuadraticListSummationFixStyle::StarredListComprehension => Ok(
            convert_to_starred_list_comprehension(iterable, call, checker),
        ),
    }
}

fn convert_to_starred_list_comprehension(
    iterable: &Expr,
    call: &ast::ExprCall,
    checker: &Checker,
) -> Fix {
    let iterable = checker.locator().slice(
        parenthesized_range(iterable.into(), (&call.arguments).into(), checker.tokens())
            .unwrap_or(iterable.range()),
    );

    Fix::unsafe_edit(Edit::range_replacement(
        format!("[*sublist for sublist in {iterable}]"),
        call.range(),
    ))
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
        parenthesized_range(iterable.into(), (&call.arguments).into(), checker.tokens())
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
    let Some(start_arg) = arguments.find_argument_value("start", 1) else {
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
