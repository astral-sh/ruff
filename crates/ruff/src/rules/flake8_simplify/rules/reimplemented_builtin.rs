use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{
    self, CmpOp, Comprehension, Constant, Expr, ExprContext, Ranged, Stmt, UnaryOp,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::source_code::Generator;

use crate::checkers::ast::Checker;
use crate::line_width::LineWidth;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for `for` loops that can be replaced with a builtin function, like
/// `any` or `all`.
///
/// ## Why is this bad?
/// Using a builtin function is more concise and readable. Builtins are also
/// more efficient than `for` loops.
///
/// ## Example
/// ```python
/// for item in iterable:
///     if predicate(item):
///         return True
/// return False
/// ```
///
/// Use instead:
/// ```python
/// return any(predicate(item) for item in iterable)
/// ```
///
/// ## References
/// - [Python documentation: `any`](https://docs.python.org/3/library/functions.html#any)
/// - [Python documentation: `all`](https://docs.python.org/3/library/functions.html#all)
#[violation]
pub struct ReimplementedBuiltin {
    repl: String,
}

impl Violation for ReimplementedBuiltin {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReimplementedBuiltin { repl } = self;
        format!("Use `{repl}` instead of `for` loop")
    }

    fn autofix_title(&self) -> Option<String> {
        let ReimplementedBuiltin { repl } = self;
        Some(format!("Replace with `{repl}`"))
    }
}

/// SIM110, SIM111
pub(crate) fn convert_for_loop_to_any_all(
    checker: &mut Checker,
    stmt: &Stmt,
    sibling: Option<&Stmt>,
) {
    // There are two cases to consider:
    // - `for` loop with an `else: return True` or `else: return False`.
    // - `for` loop followed by `return True` or `return False`
    if let Some(loop_info) = return_values_for_else(stmt)
        .or_else(|| sibling.and_then(|sibling| return_values_for_siblings(stmt, sibling)))
    {
        // Check if loop_info.target, loop_info.iter, or loop_info.test contains `await`.
        if contains_await(loop_info.target)
            || contains_await(loop_info.iter)
            || contains_await(loop_info.test)
        {
            return;
        }
        if loop_info.return_value && !loop_info.next_return_value {
            if checker.enabled(Rule::ReimplementedBuiltin) {
                let contents = return_stmt(
                    "any",
                    loop_info.test,
                    loop_info.target,
                    loop_info.iter,
                    checker.generator(),
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                let line_start = checker.locator.line_start(stmt.start());
                if LineWidth::new(checker.settings.tab_size)
                    .add_str(&checker.locator.contents()[TextRange::new(line_start, stmt.start())])
                    .add_str(&contents)
                    > checker.settings.line_length
                {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    ReimplementedBuiltin {
                        repl: contents.clone(),
                    },
                    TextRange::new(stmt.start(), loop_info.terminal),
                );
                if checker.patch(diagnostic.kind.rule()) && checker.semantic().is_builtin("any") {
                    diagnostic.set_fix(Fix::suggested(Edit::replacement(
                        contents,
                        stmt.start(),
                        loop_info.terminal,
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if !loop_info.return_value && loop_info.next_return_value {
            if checker.enabled(Rule::ReimplementedBuiltin) {
                // Invert the condition.
                let test = {
                    if let Expr::UnaryOp(ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand,
                        range: _,
                    }) = &loop_info.test
                    {
                        *operand.clone()
                    } else if let Expr::Compare(ast::ExprCompare {
                        left,
                        ops,
                        comparators,
                        range: _,
                    }) = &loop_info.test
                    {
                        if let ([op], [comparator]) = (ops.as_slice(), comparators.as_slice()) {
                            let op = match op {
                                CmpOp::Eq => CmpOp::NotEq,
                                CmpOp::NotEq => CmpOp::Eq,
                                CmpOp::Lt => CmpOp::GtE,
                                CmpOp::LtE => CmpOp::Gt,
                                CmpOp::Gt => CmpOp::LtE,
                                CmpOp::GtE => CmpOp::Lt,
                                CmpOp::Is => CmpOp::IsNot,
                                CmpOp::IsNot => CmpOp::Is,
                                CmpOp::In => CmpOp::NotIn,
                                CmpOp::NotIn => CmpOp::In,
                            };
                            let node = ast::ExprCompare {
                                left: left.clone(),
                                ops: vec![op],
                                comparators: vec![comparator.clone()],
                                range: TextRange::default(),
                            };
                            node.into()
                        } else {
                            let node = ast::ExprUnaryOp {
                                op: UnaryOp::Not,
                                operand: Box::new(loop_info.test.clone()),
                                range: TextRange::default(),
                            };
                            node.into()
                        }
                    } else {
                        let node = ast::ExprUnaryOp {
                            op: UnaryOp::Not,
                            operand: Box::new(loop_info.test.clone()),
                            range: TextRange::default(),
                        };
                        node.into()
                    }
                };
                let contents = return_stmt(
                    "all",
                    &test,
                    loop_info.target,
                    loop_info.iter,
                    checker.generator(),
                );

                // Don't flag if the resulting expression would exceed the maximum line length.
                let line_start = checker.locator.line_start(stmt.start());
                if LineWidth::new(checker.settings.tab_size)
                    .add_str(&checker.locator.contents()[TextRange::new(line_start, stmt.start())])
                    .add_str(&contents)
                    > checker.settings.line_length
                {
                    return;
                }

                let mut diagnostic = Diagnostic::new(
                    ReimplementedBuiltin {
                        repl: contents.clone(),
                    },
                    TextRange::new(stmt.start(), loop_info.terminal),
                );
                if checker.patch(diagnostic.kind.rule()) && checker.semantic().is_builtin("all") {
                    diagnostic.set_fix(Fix::suggested(Edit::replacement(
                        contents,
                        stmt.start(),
                        loop_info.terminal,
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

struct Loop<'a> {
    return_value: bool,
    next_return_value: bool,
    test: &'a Expr,
    target: &'a Expr,
    iter: &'a Expr,
    terminal: TextSize,
}

/// Extract the returned boolean values a `Stmt::For` with an `else` body.
fn return_values_for_else(stmt: &Stmt) -> Option<Loop> {
    let Stmt::For(ast::StmtFor {
        body,
        target,
        iter,
        orelse,
        ..
    }) = stmt
    else {
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
    let Stmt::If(ast::StmtIf {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
        range: _,
    }) = &body[0]
    else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = &nested_body[0] else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Bool(value),
        ..
    }) = value.as_ref()
    else {
        return None;
    };

    // The `else` block has to contain a single `return True` or `return False`.
    let Stmt::Return(ast::StmtReturn {
        value: next_value,
        range: _,
    }) = &orelse[0]
    else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Bool(next_value),
        ..
    }) = next_value.as_ref()
    else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
        terminal: stmt.end(),
    })
}

/// Extract the returned boolean values from subsequent `Stmt::For` and
/// `Stmt::Return` statements, or `None`.
fn return_values_for_siblings<'a>(stmt: &'a Stmt, sibling: &'a Stmt) -> Option<Loop<'a>> {
    let Stmt::For(ast::StmtFor {
        body,
        target,
        iter,
        orelse,
        ..
    }) = stmt
    else {
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
    let Stmt::If(ast::StmtIf {
        body: nested_body,
        test: nested_test,
        orelse: nested_orelse,
        range: _,
    }) = &body[0]
    else {
        return None;
    };
    if nested_body.len() != 1 {
        return None;
    }
    if !nested_orelse.is_empty() {
        return None;
    }
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = &nested_body[0] else {
        return None;
    };
    let Some(value) = value else {
        return None;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Bool(value),
        ..
    }) = value.as_ref()
    else {
        return None;
    };

    // The next statement has to be a `return True` or `return False`.
    let Stmt::Return(ast::StmtReturn {
        value: next_value,
        range: _,
    }) = &sibling
    else {
        return None;
    };
    let Some(next_value) = next_value else {
        return None;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Bool(next_value),
        ..
    }) = next_value.as_ref()
    else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        next_return_value: *next_value,
        test: nested_test,
        target,
        iter,
        terminal: sibling.end(),
    })
}

/// Generate a return statement for an `any` or `all` builtin comprehension.
fn return_stmt(id: &str, test: &Expr, target: &Expr, iter: &Expr, generator: Generator) -> String {
    let node = ast::ExprGeneratorExp {
        elt: Box::new(test.clone()),
        generators: vec![Comprehension {
            target: target.clone(),
            iter: iter.clone(),
            ifs: vec![],
            is_async: false,
            range: TextRange::default(),
        }],
        range: TextRange::default(),
    };
    let node1 = ast::ExprName {
        id: id.into(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node2 = ast::ExprCall {
        func: Box::new(node1.into()),
        args: vec![node.into()],
        keywords: vec![],
        range: TextRange::default(),
    };
    let node3 = ast::StmtReturn {
        value: Some(Box::new(node2.into())),
        range: TextRange::default(),
    };
    generator.stmt(&node3.into())
}

/// Return `true` if the [`Expr`] contains an `await` expression.
fn contains_await(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
}
