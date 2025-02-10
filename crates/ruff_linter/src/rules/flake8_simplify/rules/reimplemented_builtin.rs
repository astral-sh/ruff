use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal;
use ruff_python_ast::{
    self as ast, Arguments, CmpOp, Comprehension, Expr, ExprContext, Stmt, UnaryOp,
};
use ruff_python_codegen::Generator;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::fits;
use crate::line_width::LineWidthBuilder;

/// ## What it does
/// Checks for `for` loops that can be replaced with a builtin function, like
/// `any` or `all`.
///
/// ## Why is this bad?
/// Using a builtin function is more concise and readable.
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
#[derive(ViolationMetadata)]
pub(crate) struct ReimplementedBuiltin {
    replacement: String,
}

impl Violation for ReimplementedBuiltin {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReimplementedBuiltin { replacement } = self;
        format!("Use `{replacement}` instead of `for` loop")
    }

    fn fix_title(&self) -> Option<String> {
        let ReimplementedBuiltin { replacement } = self;
        Some(format!("Replace with `{replacement}`"))
    }
}

/// SIM110, SIM111
pub(crate) fn convert_for_loop_to_any_all(checker: &Checker, stmt: &Stmt) {
    if !checker.semantic().current_scope().kind.is_function() {
        return;
    }

    // The `for` loop itself must consist of an `if` with a `return`.
    let Some(loop_) = match_loop(stmt) else {
        return;
    };

    // Afterwards, there are two cases to consider:
    // - `for` loop with an `else: return True` or `else: return False`.
    // - `for` loop followed by `return True` or `return False`.
    let Some(terminal) = match_else_return(stmt).or_else(|| {
        let parent = checker.semantic().current_statement_parent()?;
        let sibling = traversal::suite(stmt, parent)?.next_sibling()?;
        match_sibling_return(stmt, sibling)
    }) else {
        return;
    };

    // Check if any of the expressions contain an `await`, `yield`, or `yield from` expression.
    // If so, turning the code into an any() or all() call would produce a SyntaxError.
    if contains_yield_like(loop_.target) || contains_yield_like(loop_.test) {
        return;
    }

    match (loop_.return_value, terminal.return_value) {
        // Replace with `any`.
        (true, false) => {
            let contents = return_stmt(
                Name::new_static("any"),
                loop_.test,
                loop_.target,
                loop_.iter,
                checker.generator(),
            );

            // Don't flag if the resulting expression would exceed the maximum line length.
            if !fits(
                &contents,
                stmt.into(),
                checker.locator(),
                checker.settings.pycodestyle.max_line_length,
                checker.settings.tab_size,
            ) {
                return;
            }

            let mut diagnostic = Diagnostic::new(
                ReimplementedBuiltin {
                    replacement: contents.to_string(),
                },
                TextRange::new(stmt.start(), terminal.stmt.end()),
            );
            if checker.semantic().has_builtin_binding("any") {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
                    contents,
                    stmt.start(),
                    terminal.stmt.end(),
                )));
            }
            checker.report_diagnostic(diagnostic);
        }
        // Replace with `all`.
        (false, true) => {
            // Invert the condition.
            let test = {
                if let Expr::UnaryOp(ast::ExprUnaryOp {
                    op: UnaryOp::Not,
                    operand,
                    range: _,
                }) = &loop_.test
                {
                    *operand.clone()
                } else if let Expr::Compare(ast::ExprCompare {
                    left,
                    ops,
                    comparators,
                    range: _,
                }) = &loop_.test
                {
                    if let ([op], [comparator]) = (&**ops, &**comparators) {
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
                            ops: Box::from([op]),
                            comparators: Box::from([comparator.clone()]),
                            range: TextRange::default(),
                        };
                        node.into()
                    } else {
                        let node = ast::ExprUnaryOp {
                            op: UnaryOp::Not,
                            operand: Box::new(loop_.test.clone()),
                            range: TextRange::default(),
                        };
                        node.into()
                    }
                } else {
                    let node = ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(loop_.test.clone()),
                        range: TextRange::default(),
                    };
                    node.into()
                }
            };
            let contents = return_stmt(
                Name::new_static("all"),
                &test,
                loop_.target,
                loop_.iter,
                checker.generator(),
            );

            // Don't flag if the resulting expression would exceed the maximum line length.
            let line_start = checker.locator().line_start(stmt.start());
            if LineWidthBuilder::new(checker.settings.tab_size)
                .add_str(
                    checker
                        .locator()
                        .slice(TextRange::new(line_start, stmt.start())),
                )
                .add_str(&contents)
                > checker.settings.pycodestyle.max_line_length
            {
                return;
            }

            let mut diagnostic = Diagnostic::new(
                ReimplementedBuiltin {
                    replacement: contents.to_string(),
                },
                TextRange::new(stmt.start(), terminal.stmt.end()),
            );
            if checker.semantic().has_builtin_binding("all") {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
                    contents,
                    stmt.start(),
                    terminal.stmt.end(),
                )));
            }
            checker.report_diagnostic(diagnostic);
        }
        _ => {}
    }
}

/// Represents a `for` loop with a conditional `return`, like:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// ```
#[derive(Debug)]
struct Loop<'a> {
    /// The `return` value of the loop.
    return_value: bool,
    /// The test condition in the loop.
    test: &'a Expr,
    /// The target of the loop.
    target: &'a Expr,
    /// The iterator of the loop.
    iter: &'a Expr,
}

/// Represents a `return` statement following a `for` loop, like:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// return False
/// ```
///
/// Or:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// else:
///     return False
/// ```
#[derive(Debug)]
struct Terminal<'a> {
    return_value: bool,
    stmt: &'a Stmt,
}

fn match_loop(stmt: &Stmt) -> Option<Loop> {
    let Stmt::For(ast::StmtFor {
        body, target, iter, ..
    }) = stmt
    else {
        return None;
    };

    // The loop itself should contain a single `if` statement, with a single `return` statement in
    // the body.
    let [Stmt::If(ast::StmtIf {
        body: nested_body,
        test: nested_test,
        elif_else_clauses: nested_elif_else_clauses,
        range: _,
    })] = body.as_slice()
    else {
        return None;
    };
    if !nested_elif_else_clauses.is_empty() {
        return None;
    }
    let [Stmt::Return(ast::StmtReturn {
        value: Some(value),
        range: _,
    })] = nested_body.as_slice()
    else {
        return None;
    };
    let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) = value.as_ref() else {
        return None;
    };

    Some(Loop {
        return_value: *value,
        test: nested_test,
        target,
        iter,
    })
}

/// If a `Stmt::For` contains an `else` with a single boolean `return`, return the [`Terminal`]
/// representing that `return`.
///
/// For example, matches the `return` in:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// return False
/// ```
fn match_else_return(stmt: &Stmt) -> Option<Terminal> {
    let Stmt::For(ast::StmtFor { orelse, .. }) = stmt else {
        return None;
    };

    // The `else` block has to contain a single `return True` or `return False`.
    let [Stmt::Return(ast::StmtReturn {
        value: Some(next_value),
        range: _,
    })] = orelse.as_slice()
    else {
        return None;
    };
    let Expr::BooleanLiteral(ast::ExprBooleanLiteral {
        value: next_value, ..
    }) = next_value.as_ref()
    else {
        return None;
    };

    Some(Terminal {
        return_value: *next_value,
        stmt,
    })
}

/// If a `Stmt::For` is followed by a boolean `return`, return the [`Terminal`] representing that
/// `return`.
///
/// For example, matches the `return` in:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// else:
///     return False
/// ```
fn match_sibling_return<'a>(stmt: &'a Stmt, sibling: &'a Stmt) -> Option<Terminal<'a>> {
    let Stmt::For(ast::StmtFor { orelse, .. }) = stmt else {
        return None;
    };

    // The loop itself shouldn't have an `else` block.
    if !orelse.is_empty() {
        return None;
    }

    // The next statement has to be a `return True` or `return False`.
    let Stmt::Return(ast::StmtReturn {
        value: Some(next_value),
        range: _,
    }) = &sibling
    else {
        return None;
    };
    let Expr::BooleanLiteral(ast::ExprBooleanLiteral {
        value: next_value, ..
    }) = next_value.as_ref()
    else {
        return None;
    };

    Some(Terminal {
        return_value: *next_value,
        stmt: sibling,
    })
}

/// Generate a return statement for an `any` or `all` builtin comprehension.
fn return_stmt(id: Name, test: &Expr, target: &Expr, iter: &Expr, generator: Generator) -> String {
    let node = ast::ExprGenerator {
        elt: Box::new(test.clone()),
        generators: vec![Comprehension {
            target: target.clone(),
            iter: iter.clone(),
            ifs: vec![],
            is_async: false,
            range: TextRange::default(),
        }],
        range: TextRange::default(),
        parenthesized: false,
    };
    let node1 = ast::ExprName {
        id,
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node2 = ast::ExprCall {
        func: Box::new(node1.into()),
        arguments: Arguments {
            args: Box::from([node.into()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    let node3 = ast::StmtReturn {
        value: Some(Box::new(node2.into())),
        range: TextRange::default(),
    };
    generator.stmt(&node3.into())
}

/// Return `true` if the [`Expr`] contains an `await`, `yield`, or `yield from` expression.
fn contains_yield_like(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
        || any_over_expr(expr, &Expr::is_yield_expr)
        || any_over_expr(expr, &Expr::is_yield_from_expr)
}
