use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced with `yield from` expressions.
///
/// ## Why is this bad?
/// If a `for` loop only contains a `yield` statement, it can be replaced with a
/// `yield from` expression, which is more concise and idiomatic.
///
/// ## Example
/// ```python
/// for x in foo:
///     yield x
/// ```
///
/// Use instead:
/// ```python
/// yield from foo
/// ```
///
/// ## References
/// - [Python documentation: The `yield` statement](https://docs.python.org/3/reference/simple_stmts.html#the-yield-statement)
/// - [PEP 380](https://peps.python.org/pep-0380/)
#[violation]
pub struct YieldInForLoop;

impl AlwaysFixableViolation for YieldInForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `yield` over `for` loop with `yield from`")
    }

    fn fix_title(&self) -> String {
        "Replace with `yield from`".to_string()
    }
}

/// UP028
pub(crate) fn yield_in_for_loop(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    // Intentionally omit async contexts.
    if checker.semantic().in_async_context() {
        return;
    }

    let ast::StmtFor {
        target,
        iter,
        body,
        orelse,
        is_async: _,
        range: _,
    } = stmt_for;

    // If there is an else statement, don't rewrite.
    if !orelse.is_empty() {
        return;
    }

    // If there's any logic besides a yield, don't rewrite.
    let [body] = body.as_slice() else {
        return;
    };

    // If the body is not a yield, don't rewrite.
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = &body else {
        return;
    };
    let Expr::Yield(ast::ExprYield {
        value: Some(value),
        range: _,
    }) = value.as_ref()
    else {
        return;
    };

    // If the target is not the same as the value, don't rewrite. For example, we should rewrite
    // `for x in y: yield x` to `yield from y`, but not `for x in y: yield x + 1`.
    if !is_same_expr(target, value) {
        return;
    }

    // If any of the bound names are used outside of the yield itself, don't rewrite.
    if collect_names(value).any(|name| {
        checker
            .semantic()
            .current_scope()
            .get_all(name.id.as_str())
            .any(|binding_id| {
                let binding = checker.semantic().binding(binding_id);
                binding.references.iter().any(|reference_id| {
                    checker.semantic().reference(*reference_id).range() != name.range()
                })
            })
    }) {
        return;
    }

    let mut diagnostic = Diagnostic::new(YieldInForLoop, stmt_for.range());
    let contents = checker.locator().slice(
        parenthesized_range(
            iter.as_ref().into(),
            stmt_for.into(),
            checker.indexer().comment_ranges(),
            checker.locator().contents(),
        )
        .unwrap_or(iter.range()),
    );
    let contents = format!("yield from {contents}");
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        contents,
        stmt_for.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the two expressions are equivalent, and both consistent solely
/// of tuples and names.
fn is_same_expr(left: &Expr, right: &Expr) -> bool {
    match (&left, &right) {
        (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
        (Expr::Tuple(left), Expr::Tuple(right)) => {
            left.elts.len() == right.elts.len()
                && left
                    .elts
                    .iter()
                    .zip(right.elts.iter())
                    .all(|(left, right)| is_same_expr(left, right))
        }
        _ => false,
    }
}

/// Collect all named variables in an expression consisting solely of tuples and
/// names.
fn collect_names<'a>(expr: &'a Expr) -> Box<dyn Iterator<Item = &ast::ExprName> + 'a> {
    Box::new(
        expr.as_name_expr().into_iter().chain(
            expr.as_tuple_expr()
                .into_iter()
                .flat_map(|tuple| tuple.elts.iter().flat_map(collect_names)),
        ),
    )
}
