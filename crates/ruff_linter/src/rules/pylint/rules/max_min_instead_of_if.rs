use std::ops::Deref;

use ast::LiteralExpressionRef;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, CmpOp, Expr, ExprAttribute, ExprContext, Stmt};
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
    if !match_left(left, body_target) {
        return;
    }
    if !match_right(right_statement, body_value) {
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
    if !match_left(left, body_target) {
        return;
    }
    if !match_right(right_statement, body_value) {
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

fn match_left(left: &Expr, body_target: &Expr) -> bool {
    // Check that the assignments are on the same variable
    if left.is_name_expr() && body_target.is_name_expr() {
        let Some(left_operand) = left.as_name_expr() else {
            return false;
        };
        let Some(target_assignation) = body_target.as_name_expr() else {
            return false;
        };
        return left_operand.id == target_assignation.id;
    }

    if left.is_attribute_expr() && body_target.is_attribute_expr() {
        let Some(left_operand) = left.as_attribute_expr() else {
            return false;
        };
        let Some(target_assignation) = body_target.as_attribute_expr() else {
            return false;
        };
        return match_attributes(left_operand, target_assignation);
    }

    false
}

fn match_right(right_statement: &Expr, body_value: &Expr) -> bool {
    // Verify that the right part of the statements are the same.
    if right_statement.is_name_expr() && body_value.is_name_expr() {
        let Some(right_statement_value) = right_statement.as_name_expr() else {
            return false;
        };
        let Some(body_value_value) = body_value.as_name_expr() else {
            return false;
        };
        return right_statement_value.id == body_value_value.id;
    }
    if right_statement.is_literal_expr() && body_value.is_literal_expr() {
        let Some(right_statement_value) = right_statement.as_literal_expr() else {
            return false;
        };
        let Some(body_value_value) = body_value.as_literal_expr() else {
            return false;
        };
        match (right_statement_value, body_value_value) {
            (
                LiteralExpressionRef::BytesLiteral(ast::ExprBytesLiteral { value: value1, .. }),
                LiteralExpressionRef::BytesLiteral(ast::ExprBytesLiteral { value: value2, .. }),
            ) => {
                return value1
                    .iter()
                    .map(ruff_python_ast::BytesLiteral::as_slice)
                    .eq(value2.iter().map(ruff_python_ast::BytesLiteral::as_slice))
            }
            (
                LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral {
                    value: value1, ..
                }),
                LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral {
                    value: value2, ..
                }),
            ) => return value1.to_str() == value2.to_str(),
            (
                LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral {
                    value: value1, ..
                }),
                LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral {
                    value: value2, ..
                }),
            ) => return value1 == value2,
            (_, _) => return false,
        }
    }
    false
}

fn match_attributes(expr1: &ExprAttribute, expr2: &ExprAttribute) -> bool {
    if expr1.attr.as_str() != expr2.attr.as_str() {
        return false;
    }

    if expr1.value.is_name_expr() && expr2.value.is_name_expr() {
        let Some(ast::ExprName { id: id1, .. }) = expr1.value.as_name_expr() else {
            return false;
        };
        let Some(ast::ExprName { id: id2, .. }) = expr2.value.as_name_expr() else {
            return false;
        };
        return id1 == id2;
    }

    if expr1.value.is_attribute_expr() && expr2.value.is_attribute_expr() {
        let Some(expr1) = expr1.value.as_attribute_expr() else {
            return false;
        };
        let Some(expr2) = expr2.value.as_attribute_expr() else {
            return false;
        };
        return match_attributes(expr1, expr2);
    }

    false
}
