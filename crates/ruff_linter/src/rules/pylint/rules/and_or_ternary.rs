use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{BoolOp, Expr, ExprBoolOp, ExprIfExp};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pylint::helpers::is_and_or_ternary;

/// ## What it does
/// Checks if pre-python 2.5 ternary syntax is used.
///
/// ## Why is this bad?
/// If-expression is more readable than logical ternary idiom.
///
/// ## Example
/// ```python
/// x, y = 1, 2
/// maximum = x >= y and x or y  # [consider-using-ternary]
/// ```
///
/// Use instead:
/// ```python
/// x, y = 1, 2
/// maximum = x if x >= y else y
/// ```
#[violation]
pub struct AndOrTernary {
    ternary: String,
}

impl Violation for AndOrTernary {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let AndOrTernary { ternary } = self;
        format!("Consider using ternary `{ternary}`")
    }

    fn fix_title(&self) -> Option<String> {
        let AndOrTernary { ternary } = self;
        Some(format!("Use `{ternary}`"))
    }
}

pub(crate) fn and_or_ternary(checker: &mut Checker, bool_op: &ExprBoolOp) {
    if !is_and_or_ternary(bool_op) {
        return;
    }

    let if_expr = Expr::IfExp(ExprIfExp {
        test: Box::new(bool_op.values[0].as_bool_op_expr().unwrap().values[0].clone()),
        body: Box::new(bool_op.values[0].as_bool_op_expr().unwrap().values[1].clone()),
        orelse: Box::new(if bool_op.values.len() == 2 {
            bool_op.values[1].clone()
        } else {
            Expr::BoolOp(ExprBoolOp {
                op: BoolOp::Or,
                values: bool_op.values[1..].to_vec(),
                range: TextRange::default(),
            })
        }),
        range: TextRange::default(),
    });
    let ternary = checker.generator().expr(&if_expr);

    let mut diagnostic = Diagnostic::new(
        AndOrTernary {
            ternary: ternary.clone(),
        },
        bool_op.range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            ternary,
            bool_op.range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}
