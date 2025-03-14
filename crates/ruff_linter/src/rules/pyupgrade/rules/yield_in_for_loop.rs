use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
///
/// global y
/// for y in foo:
///     yield y
/// ```
///
/// Use instead:
/// ```python
/// yield from foo
///
/// for _element in foo:
///     y = _element
///     yield y
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as converting a `for` loop to a `yield
/// from` expression can change the behavior of the program in rare cases.
/// For example, if a generator is being sent values via `send`, then rewriting
/// to a `yield from` could lead to an attribute error if the underlying
/// generator does not implement the `send` method.
///
/// Additionally, if at least one target is `global` or `nonlocal`,
/// no fix will be offered.
///
/// In most cases, however, the fix is safe, and such a modification should have
/// no effect on the behavior of the program.
///
/// ## References
/// - [Python documentation: The `yield` statement](https://docs.python.org/3/reference/simple_stmts.html#the-yield-statement)
/// - [PEP 380 – Syntax for Delegating to a Subgenerator](https://peps.python.org/pep-0380/)
#[derive(ViolationMetadata)]
pub(crate) struct YieldInForLoop;

impl Violation for YieldInForLoop {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace `yield` over `for` loop with `yield from`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `yield from`".to_string())
    }
}

/// UP028
pub(crate) fn yield_in_for_loop(checker: &Checker, stmt_for: &ast::StmtFor) {
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
            // Skip unbound bindings like `del x`
            .find(|&id| !checker.semantic().binding(id).is_unbound())
            .is_some_and(|binding_id| {
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
            checker.comment_ranges(),
            checker.locator().contents(),
        )
        .unwrap_or(iter.range()),
    );
    let contents = if iter.as_tuple_expr().is_some_and(|it| !it.parenthesized) {
        format!("yield from ({contents})")
    } else {
        format!("yield from {contents}")
    };

    if !collect_names(value).any(|name| {
        let semantic = checker.semantic();
        let mut bindings = semantic.current_scope().get_all(name.id.as_str());

        bindings.any(|id| {
            let binding = semantic.binding(id);

            binding.is_global() || binding.is_nonlocal()
        })
    }) {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_for.range(),
        )));
    }

    checker.report_diagnostic(diagnostic);
}

/// Return `true` if the two expressions are equivalent, and both consistent solely
/// of tuples and names.
fn is_same_expr(left: &Expr, right: &Expr) -> bool {
    match (&left, &right) {
        (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
        (Expr::Tuple(left), Expr::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| is_same_expr(left, right))
        }
        _ => false,
    }
}

/// Collect all named variables in an expression consisting solely of tuples and
/// names.
fn collect_names<'a>(expr: &'a Expr) -> Box<dyn Iterator<Item = &'a ast::ExprName> + 'a> {
    Box::new(
        expr.as_name_expr().into_iter().chain(
            expr.as_tuple_expr()
                .into_iter()
                .flat_map(|tuple| tuple.iter().flat_map(collect_names)),
        ),
    )
}
