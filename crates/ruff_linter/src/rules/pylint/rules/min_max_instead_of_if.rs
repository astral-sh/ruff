use std::ops::Deref;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Arguments, CmpOp, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for an if node that can be refactored as a min/max python builtin.
///
/// ## Why is this bad?
/// An if block where the test and assignment have the same structure can
/// be expressed more concisely by using the python builtin min/max function.
///
/// ## Example
/// ```python
/// if value < 10:
///     value = 10
/// ```
///
/// Use instead:
/// ```python
/// value = max(value, 10)
/// ```
///
/// ## References
/// - [Python documentation: max function](https://docs.python.org/3/library/functions.html#max)
/// - [Python documentation: min function](https://docs.python.org/3/library/functions.html#min)
#[violation]
pub struct MinMaxInsteadOfIf {
    contents: String,
}

impl Violation for MinMaxInsteadOfIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MinMaxInsteadOfIf { contents } = self;
        format!("Consider using `{contents}` instead of unnecessary if block")
    }
}

/// R1730 (and also R1731)
pub(crate) fn min_max_instead_of_if(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    } = stmt_if;

    if !elif_else_clauses.is_empty() {
        return;
    }

    let [Stmt::Assign(ast::StmtAssign {
        targets: body_targets,
        value: body_value,
        ..
    })] = body.as_slice()
    else {
        return;
    };
    let [body_target] = body_targets.as_slice() else {
        return;
    };

    let Some(ast::ExprCompare {
        ops,
        left,
        comparators,
        ..
    }) = test.as_compare_expr()
    else {
        return;
    };

    if !(!body_target.is_subscript_expr() && !left.is_subscript_expr()) {
        return;
    }

    let ([op], [right_statement]) = (&**ops, &**comparators) else {
        return;
    };

    let min_or_max = match op {
        CmpOp::Gt | CmpOp::GtE => MinMax::Min,
        CmpOp::Lt | CmpOp::LtE => MinMax::Max,
        _ => return,
    };

    let left_cmp = ComparableExpr::from(left);
    let body_target_cmp = ComparableExpr::from(body_target);
    let right_statement_cmp = ComparableExpr::from(right_statement);
    let body_value_cmp = ComparableExpr::from(body_value);
    if left_cmp != body_target_cmp || right_statement_cmp != body_value_cmp {
        return;
    }

    let func_node = ast::ExprName {
        id: min_or_max.as_str().into(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let value_node = ast::ExprCall {
        func: Box::new(func_node.into()),
        arguments: Arguments {
            args: Box::from([body_target.clone(), body_value.deref().clone()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    let assign_node = ast::StmtAssign {
        targets: vec![body_target.clone()],
        value: Box::new(value_node.into()),
        range: TextRange::default(),
    };
    let diagnostic = Diagnostic::new(
        MinMaxInsteadOfIf {
            contents: checker.generator().stmt(&assign_node.into()),
        },
        stmt_if.range(),
    );
    checker.diagnostics.push(diagnostic);
}

enum MinMax {
    Min,
    Max,
}

impl MinMax {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}
