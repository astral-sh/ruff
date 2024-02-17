use std::ops::Deref;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Arguments, CmpOp, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for an if node that can be refactored as a max python builtin.
///
/// ## Why is this bad?
/// An if block where the test and assignment have the same structure can
/// be expressed more concisely by using the python builtin max function.
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
#[violation]
pub struct MaxInsteadOfIf {
    contents: String,
}

impl Violation for MaxInsteadOfIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MaxInsteadOfIf { contents } = self;
        format!("Consider using `{contents}` instead of unnecessary if block")
    }
}

/// ## What it does
/// Check for an if node that can be refactored as a min python builtin.
///
/// ## Why is this bad?
/// An if block where the test and assignment have the same structure can
/// be expressed more concisely by using the python builtin min function.
///
/// ## Example
/// ```python
/// if value > 10:
///     value = 10
/// ```
///
/// Use instead:
/// ```python
/// value = min(value, 10)
/// ```
///
/// ## References
/// - [Python documentation: min function](https://docs.python.org/3/library/functions.html#min)
#[violation]
pub struct MinInsteadOfIf {
    contents: String,
}

impl Violation for MinInsteadOfIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MinInsteadOfIf { contents } = self;
        format!("Consider using `{contents}` instead of unnecessary if block")
    }
}

/// R1730
pub(crate) fn min_instead_of_if(checker: &mut Checker, stmt_if: &ast::StmtIf) {
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

    if !matches!(op, CmpOp::Gt | CmpOp::GtE) {
        return;
    }

    let left_cmp = ComparableExpr::from(left);
    let body_target_cmp = ComparableExpr::from(body_target);
    let right_statement_cmp = ComparableExpr::from(right_statement);
    let body_value_cmp = ComparableExpr::from(body_value);
    if left_cmp != body_target_cmp || right_statement_cmp != body_value_cmp {
        return;
    }

    let func_node = ast::ExprName {
        id: "min".into(),
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
        MinInsteadOfIf {
            contents: checker.generator().stmt(&assign_node.into()),
        },
        stmt_if.range(),
    );
    checker.diagnostics.push(diagnostic);
}

/// R1731
pub(crate) fn max_instead_of_if(checker: &mut Checker, stmt_if: &ast::StmtIf) {
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

    if !matches!(op, CmpOp::Lt | CmpOp::LtE) {
        return;
    }

    let left_cmp = ComparableExpr::from(left);
    let body_target_cmp = ComparableExpr::from(body_target);
    let right_statement_cmp = ComparableExpr::from(right_statement);
    let body_value_cmp = ComparableExpr::from(body_value);
    if left_cmp != body_target_cmp || right_statement_cmp != body_value_cmp {
        return;
    }

    let func_node = ast::ExprName {
        id: "max".into(),
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
        MaxInsteadOfIf {
            contents: checker.generator().stmt(&assign_node.into()),
        },
        stmt_if.range(),
    );
    checker.diagnostics.push(diagnostic);
}
