use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## Deprecation
/// There's no [evidence](https://github.com/astral-sh/ruff/issues/12754) that generators are
/// meaningfully faster than list comprehensions when combined with unpacking.
///
/// ## What it does
/// Checks for list comprehensions that are immediately unpacked.
///
/// ## Why is this bad?
/// There is no reason to use a list comprehension if the result is immediately
/// unpacked. Instead, use a generator expression, which avoids allocating
/// an intermediary list.
///
/// ## Example
/// ```python
/// a, b, c = [foo(x) for x in items]
/// ```
///
/// Use instead:
/// ```python
/// a, b, c = (foo(x) for x in items)
/// ```
///
/// ## References
/// - [Python documentation: Generator expressions](https://docs.python.org/3/reference/expressions.html#generator-expressions)
/// - [Python documentation: List comprehensions](https://docs.python.org/3/tutorial/datastructures.html#list-comprehensions)
#[violation]
pub struct UnpackedListComprehension;

impl AlwaysFixableViolation for UnpackedListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace unpacked list comprehension with a generator expression")
    }

    fn fix_title(&self) -> String {
        "Replace with generator expression".to_string()
    }
}

/// UP027
pub(crate) fn unpacked_list_comprehension(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let Some(target) = targets.first() else {
        return;
    };

    if !target.is_tuple_expr() {
        return;
    }

    let Expr::ListComp(ast::ExprListComp {
        elt,
        generators,
        range: _,
    }) = value
    else {
        return;
    };

    if generators.iter().any(|generator| generator.is_async) || contains_await(elt) {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnpackedListComprehension, value.range());
    let existing = checker.locator().slice(value);

    let mut content = String::with_capacity(existing.len());
    content.push('(');
    content.push_str(&existing[1..existing.len() - 1]);
    content.push(')');
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        content,
        value.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the [`Expr`] contains an `await` expression.
fn contains_await(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
}
