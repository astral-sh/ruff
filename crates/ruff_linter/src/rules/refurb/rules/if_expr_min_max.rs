use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Arguments, CmpOp, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for `if` expressions that can be replaced with `min()` or `max()` calls.
///
/// ## Why is this bad?
/// `if` expressions for choosing the lesser or greater of two expressions can be replaced with
/// `min()` or `max()` calls, which are more concise and readable.
///
/// ## Example
/// ```python
/// highest_score = score1 if score1 > score2 else score2
/// ```
///
/// Use instead:
/// ```python
/// highest_score = max(score2, score1)
/// ```
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3.11/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3.11/library/functions.html#max)
#[violation]
pub struct IfExprMinMax {
    expr: String,
    repl: String,
}

impl AlwaysFixableViolation for IfExprMinMax {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprMinMax { expr, repl } = self;
        format!("Replace `{expr}` with `{repl}`")
    }

    fn fix_title(&self) -> String {
        let IfExprMinMax { repl, .. } = self;
        format!("Replace with `{repl}`")
    }
}

/// FURB136
pub(crate) fn if_expr_min_max(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    // Ignore, e.g., `foo < bar < baz`.
    let [op] = ops.as_slice() else {
        return;
    };

    let (mut use_max, mut flip_args) = match op {
        CmpOp::Gt => (true, true),
        CmpOp::GtE => (true, false),
        CmpOp::Lt => (false, true),
        CmpOp::LtE => (false, false),
        _ => return,
    };

    let [right] = comparators.as_slice() else {
        return;
    };

    let body_cmp = ComparableExpr::from(body);
    let orelse_cmp = ComparableExpr::from(orelse);
    let left_cmp = ComparableExpr::from(left);
    let right_cmp = ComparableExpr::from(right);

    if body_cmp == right_cmp && orelse_cmp == left_cmp {
        use_max = !use_max;
        flip_args = !flip_args;
    } else if body_cmp != left_cmp || orelse_cmp != right_cmp {
        return;
    }

    let func = if use_max { "max" } else { "min" };
    let (arg1, arg2) = if flip_args {
        (right, left.as_ref())
    } else {
        (left.as_ref(), right)
    };

    let mut diagnostic = Diagnostic::new(
        IfExprMinMax {
            expr: checker.generator().expr(expr),
            repl: format!(
                "{func}({}, {})",
                checker.generator().expr(arg1),
                checker.generator().expr(arg2),
            ),
        },
        expr.range(),
    );

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(
            &ast::ExprCall {
                func: Box::new(
                    ast::ExprName {
                        id: func.into(),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    }
                    .into(),
                ),
                arguments: Arguments {
                    args: vec![arg1.clone(), arg2.clone()],
                    keywords: vec![],
                    range: TextRange::default(),
                },
                range: TextRange::default(),
            }
            .into(),
        ),
        expr.range(),
    )));

    checker.diagnostics.push(diagnostic);
}
