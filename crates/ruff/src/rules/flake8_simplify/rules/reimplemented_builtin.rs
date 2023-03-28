use rustpython_parser::ast::{
    Cmpop, Comprehension, Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind, Unaryop,
};
use unicode_width::UnicodeWidthStr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, create_stmt, unparse_stmt};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct ReimplementedBuiltin {
    pub repl: String,
}

impl AlwaysAutofixableViolation for ReimplementedBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ReimplementedBuiltin { repl } = self;
        format!("Use `{repl}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ReimplementedBuiltin { repl } = self;
        format!("Replace with `{repl}`")
    }
}

struct Loop<'a> {
    return_value: bool,
    next_return_value: bool,
    test: &'a Expr,
    target: &'a Expr,
    iter: &'a Expr,
    terminal: Location,
}

/// Extract the returned boolean values a `StmtKind::For` with an `else` body.
fn return_values_for_else(stmt: &Stmt) -> Option<Loop> {
    let StmtKind::For {
        body,
        target,
        iter,
        orelse,
        ..
    } = &stmt.node else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with an `else`
    // containing a single `return True` or `return False`.
    if body.len() != 1 {
        return None;
    }
    if orelse.len() != 1 {
        return None;
    }
    let StmtKind::If {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
    } = &body[0].node else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let StmtKind::Return { value } = &nested_body[0].node else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(value), .. } = &value.node else {
        return None;
    };

    // The `else` block has to contain a single `return True` or `return False`.
    let StmtKind::Return { value: next_value } = &orelse[0].node else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(next_value), .. } = &next_value.node else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
        terminal: stmt.end_location.unwrap(),
    })
}

/// Extract the returned boolean values from subsequent `StmtKind::For` and
/// `StmtKind::Return` statements, or `None`.
fn return_values_for_siblings<'a>(stmt: &'a Stmt, sibling: &'a Stmt) -> Option<Loop<'a>> {
    let StmtKind::For {
        body,
        target,
        iter,
        orelse,
        ..
    } = &stmt.node else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with a single `return
    // True` or `return False`.
    if body.len() != 1 {
        return None;
    }
    if !orelse.is_empty() {
        return None;
    }
    let StmtKind::If {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
    } = &body[0].node else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let StmtKind::Return { value } = &nested_body[0].node else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(value), .. } = &value.node else {
        return None;
    };

    // The next statement has to be a `return True` or `return False`.
    let StmtKind::Return { value: next_value } = &sibling.node else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let ExprKind::Constant { value: Constant::Bool(next_value), .. } = &next_value.node else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
        terminal: sibling.end_location.unwrap(),
    })
}

/// Generate a return statement for an `any` or `all` builtin comprehension.
fn return_stmt(id: &str, test: &Expr, target: &Expr, iter: &Expr, stylist: &Stylist) -> String {
    unparse_stmt(
        &create_stmt(StmtKind::Return {
            value: Some(Box::new(create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Name {
                    id: id.to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![create_expr(ExprKind::GeneratorExp {
                    elt: Box::new(test.clone()),
                    generators: vec![Comprehension {
                        target: target.clone(),
                        iter: iter.clone(),
                        ifs: vec![],
                        is_async: 0,
                    }],
                })],
                keywords: vec![],
            }))),
        }),
        stylist,
    )
}

/// SIM110, SIM111
pub fn convert_for_loop_to_any_all(checker: &mut Checker, stmt: &Stmt, sibling: Option<&Stmt>) {
    // There are two cases to consider:
    // - `for` loop with an `else: return True` or `else: return False`.
    // - `for` loop followed by `return True` or `return False`
    if let Some(loop_info) = return_values_for_else(stmt)
        .or_else(|| sibling.and_then(|sibling| return_values_for_siblings(stmt, sibling)))
    {
        if loop_info.return_value && !loop_info.next_return_value {
            if checker.settings.rules.enabled(Rule::ReimplementedBuiltin) {
                let contents = return_stmt(
                    "any",
                    loop_info.test,
                    loop_info.target,
                    loop_info.iter,
                    checker.stylist,
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                if stmt.location.column() + contents.width() > checker.settings.line_length {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    ReimplementedBuiltin {
                        repl: contents.clone(),
                    },
                    Range::from(stmt),
                );
                if checker.patch(diagnostic.kind.rule()) && checker.ctx.is_builtin("any") {
                    diagnostic.set_fix(Edit::replacement(
                        contents,
                        stmt.location,
                        loop_info.terminal,
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if !loop_info.return_value && loop_info.next_return_value {
            if checker.settings.rules.enabled(Rule::ReimplementedBuiltin) {
                // Invert the condition.
                let test = {
                    if let ExprKind::UnaryOp {
                        op: Unaryop::Not,
                        operand,
                    } = &loop_info.test.node
                    {
                        *operand.clone()
                    } else if let ExprKind::Compare {
                        left,
                        ops,
                        comparators,
                    } = &loop_info.test.node
                    {
                        if ops.len() == 1 && comparators.len() == 1 {
                            let op = match &ops[0] {
                                Cmpop::Eq => Cmpop::NotEq,
                                Cmpop::NotEq => Cmpop::Eq,
                                Cmpop::Lt => Cmpop::GtE,
                                Cmpop::LtE => Cmpop::Gt,
                                Cmpop::Gt => Cmpop::LtE,
                                Cmpop::GtE => Cmpop::Lt,
                                Cmpop::Is => Cmpop::IsNot,
                                Cmpop::IsNot => Cmpop::Is,
                                Cmpop::In => Cmpop::NotIn,
                                Cmpop::NotIn => Cmpop::In,
                            };
                            create_expr(ExprKind::Compare {
                                left: left.clone(),
                                ops: vec![op],
                                comparators: vec![comparators[0].clone()],
                            })
                        } else {
                            create_expr(ExprKind::UnaryOp {
                                op: Unaryop::Not,
                                operand: Box::new(loop_info.test.clone()),
                            })
                        }
                    } else {
                        create_expr(ExprKind::UnaryOp {
                            op: Unaryop::Not,
                            operand: Box::new(loop_info.test.clone()),
                        })
                    }
                };
                let contents = return_stmt(
                    "all",
                    &test,
                    loop_info.target,
                    loop_info.iter,
                    checker.stylist,
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                if stmt.location.column() + contents.width() > checker.settings.line_length {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    ReimplementedBuiltin {
                        repl: contents.clone(),
                    },
                    Range::from(stmt),
                );
                if checker.patch(diagnostic.kind.rule()) && checker.ctx.is_builtin("all") {
                    diagnostic.set_fix(Edit::replacement(
                        contents,
                        stmt.location,
                        loop_info.terminal,
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
