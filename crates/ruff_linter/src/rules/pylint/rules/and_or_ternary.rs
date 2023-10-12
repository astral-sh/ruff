use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    BoolOp, Expr, ExprBoolOp, ExprDictComp, ExprIfExp, ExprListComp, ExprSetComp,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks if pre-python 2.5 ternary syntax is used.
///
/// ## Why is this bad?
/// If-expressions are more readable than logical ternary expressions.
///
/// ## Example
/// ```python
/// x, y = 1, 2
/// maximum = x >= y and x or y
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
        format!("Pre-python 2.5 ternary syntax used")
    }

    fn fix_title(&self) -> Option<String> {
        let AndOrTernary { ternary } = self;
        Some(format!("Use `{ternary}`"))
    }
}

/// Returns `Some((condition, true_value, false_value))`
/// if `bool_op` is `condition and true_value or false_value` form.
fn parse_and_or_ternary(bool_op: &ExprBoolOp) -> Option<(Expr, Expr, Expr)> {
    if bool_op.op != BoolOp::Or {
        return None;
    }
    let [expr, false_value] = bool_op.values.as_slice() else {
        return None;
    };
    let Some(and_op) = expr.as_bool_op_expr() else {
        return None;
    };
    if and_op.op != BoolOp::And {
        return None;
    }
    let [condition, true_value] = and_op.values.as_slice() else {
        return None;
    };
    if !false_value.is_bool_op_expr() && !true_value.is_bool_op_expr() {
        return Some((condition.clone(), true_value.clone(), false_value.clone()));
    }
    None
}

/// Returns `true` if expr is used as comprehension-if.
pub fn is_comprehension_if(parent: Option<&Expr>, expr: &ExprBoolOp) -> bool {
    let mut comprehensions;
    match parent {
        Some(Expr::ListComp(ExprListComp { generators, .. })) => {
            comprehensions = generators;
        }
        Some(Expr::SetComp(ExprSetComp { generators, .. })) => {
            comprehensions = generators;
        }
        Some(Expr::DictComp(ExprDictComp { generators, .. })) => {
            comprehensions = generators;
        }
        _ => {
            return false;
        }
    }
    comprehensions
        .iter()
        .any(|comp| comp.ifs.iter().any(|ifs| ifs.range() == expr.range()))
}

pub(crate) fn and_or_ternary(checker: &mut Checker, bool_op: &ExprBoolOp) {
    if checker.semantic().current_statement().is_if_stmt() {
        return;
    }
    let parent_expr = checker.semantic().current_expression_parent();
    if parent_expr.is_some_and(Expr::is_bool_op_expr) {
        return;
    }
    let Some((condition, true_value, false_value)) = parse_and_or_ternary(bool_op) else {
        return;
    };

    let if_expr = Expr::IfExp(ExprIfExp {
        test: Box::new(condition),
        body: Box::new(true_value),
        orelse: Box::new(false_value),
        range: TextRange::default(),
    });

    let ternary = if is_comprehension_if(parent_expr, bool_op) {
        format!("({})", checker.generator().expr(&if_expr))
    } else {
        checker.generator().expr(&if_expr)
    };

    let mut diagnostic = Diagnostic::new(
        AndOrTernary {
            ternary: ternary.clone(),
        },
        bool_op.range,
    );
    if checker.enabled(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            ternary,
            bool_op.range,
        )));
    }
    checker.diagnostics.push(diagnostic);
}
