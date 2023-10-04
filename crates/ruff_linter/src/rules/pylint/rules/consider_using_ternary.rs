use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{BoolOp, Expr, ExprBoolOp, ExprIfExp};
use ruff_python_parser::parse_expression;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What is does
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
pub struct ConsiderUsingTernary {
    ternary: String,
}

impl Violation for ConsiderUsingTernary {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingTernary { ternary } = self;
        format!("Consider using ternary `{ternary}`")
    }

    fn fix_title(&self) -> Option<String> {
        let ConsiderUsingTernary { ternary } = self;
        Some(format!("Use `{ternary}`"))
    }
}

pub(crate) fn consider_using_ternary(
    checker: &mut Checker,
    bool_op: &ExprBoolOp,
) {
    if !is_legacy_ternary(bool_op) {
        return;
    }

    let if_expr = Expr::IfExp(
        ExprIfExp {
            test: Box::new(bool_op.values[0].as_bool_op_expr().unwrap().values[0].clone()),
            body: Box::new(bool_op.values[0].as_bool_op_expr().unwrap().values[1].clone()),
            orelse: Box::new(if bool_op.values.len() == 2 { bool_op.values[1].clone() } else {
                Expr::BoolOp(
                    ExprBoolOp {
                        op: BoolOp::Or,
                        values: bool_op.values[1..].to_vec(),
                        range: TextRange::default(),
                    }
                )
            }),
            range: TextRange::default(),
        }
    );
    let ternary = checker.generator().expr(&if_expr);

    let mut diagnostic = Diagnostic::new(
        ConsiderUsingTernary { ternary: ternary.clone() }, bool_op.range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            ternary,
            bool_op.range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}

/// Return `true` if `bool_op` is `condition and expr_if_true or expr_if_false`
fn is_legacy_ternary(bool_op: &ExprBoolOp) -> bool {
    bool_op.op == BoolOp::Or
        && bool_op.values.len() >= 2
        && bool_op.values[0].is_bool_op_expr()
        && bool_op.values[0].as_bool_op_expr().unwrap().op == BoolOp::And
}

#[allow(dead_code)]
fn parse_bool_op(s: &str) -> Option<ExprBoolOp> {
    if let Ok(expr) = parse_expression(s, "<embedded>") {
        expr.as_bool_op_expr().cloned()
    } else {
        None
    }
}

#[allow(dead_code)]
fn is_str_legacy_ternary(s: &str) -> bool {
    if let Some(bool_op) = parse_bool_op(s) {
        is_legacy_ternary(&bool_op)
    } else {
        false
    }
}

#[test]
fn test_is_legacy_ternary() {
    // positive
    assert!(is_str_legacy_ternary("1<2 and 'a' or 'b'"));
    assert!(is_str_legacy_ternary("1<2 and 'a' or 'b' and 'd'"));  // 'a' if 1<2 else 'b' and 'd'
    assert!(is_str_legacy_ternary("1<2 and 'a' or 'b' or 'd'"));  // 'a' if 1<2 else 'b' or 'd'

    // negative
    assert!(!is_str_legacy_ternary("1<2 and 'a'"));
    assert!(!is_str_legacy_ternary("1<2 or 'a'"));
    assert!(!is_str_legacy_ternary("2>1 or 'a' and 'b'"));
    assert!(!is_str_legacy_ternary("2>1"));
    assert!(!is_str_legacy_ternary("'string'"));
}
