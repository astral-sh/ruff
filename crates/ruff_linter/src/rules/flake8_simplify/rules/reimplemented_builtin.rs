use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{
    BuildId, Builder, Expr, ExprFields, ExprKind, ExprUnaryOpFields, ListId, Stmt, StmtFields,
    StmtIfFields, StmtKind, StmtReturnFields, UnaryOp, build, traversal,
};
use ruff_python_codegen::Generator;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::fits;
use crate::fix::syntax;
use crate::line_width::LineWidthBuilder;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `for` loops that can be replaced with a builtin function, like
/// `any` or `all`.
///
/// ## Why is this bad?
/// Using a builtin function is more concise and readable.
///
/// ## Example
/// ```python
/// def foo():
///     for item in iterable:
///         if predicate(item):
///             return True
///     return False
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     return any(predicate(item) for item in iterable)
/// ```
///
/// ## Fix safety
///
/// This fix is always marked as unsafe because it might remove comments.
///
/// ## Options
///
/// The rule will avoid flagging cases where using the builtin function would exceed the configured
/// line length, as determined by these options:
///
/// - `lint.pycodestyle.max-line-length`
/// - `indent-width`
///
/// ## References
/// - [Python documentation: `any`](https://docs.python.org/3/library/functions.html#any)
/// - [Python documentation: `all`](https://docs.python.org/3/library/functions.html#all)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.211", category = Category::Complexity)]
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
pub(crate) fn convert_for_loop_to_any_all(checker: &Checker, stmt: Stmt<'_>) {
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
            let mut builder = Builder::default();
            let test = builder.copy_node(loop_.test);
            let contents = return_stmt(
                &mut builder,
                "any",
                test,
                loop_.target,
                loop_.iter,
                checker.generator(),
            );

            // Don't flag if the resulting expression would exceed the maximum line length.
            if !fits(
                &contents,
                stmt.into(),
                checker.locator(),
                checker.settings().pycodestyle.max_line_length,
                checker.settings().tab_size,
            ) {
                return;
            }

            let mut diagnostic = checker.report_diagnostic(
                ReimplementedBuiltin {
                    replacement: contents.clone(),
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
        }
        // Replace with `all`.
        (false, true) => {
            let mut builder = Builder::default();
            // Invert the condition.
            let test = {
                if let ExprFields::UnaryOp(ExprUnaryOpFields {
                    op: UnaryOp::Not,
                    operand,
                    range: _,
                    node: _,
                }) = loop_.test.fields()
                {
                    builder.copy_node(operand)
                } else if let ExprKind::Compare(compare) = loop_.test.classify()
                    && let Some((_, op, _)) = compare.as_single()
                {
                    syntax::with_comparison_op(&mut builder, compare, op.negate())
                } else {
                    syntax::not_expr(&mut builder, loop_.test)
                }
            };
            let contents = return_stmt(
                &mut builder,
                "all",
                test,
                loop_.target,
                loop_.iter,
                checker.generator(),
            );
            // Don't flag if the resulting expression would exceed the maximum line length.
            let line_start = checker.locator().line_start(stmt.start());
            if LineWidthBuilder::new(checker.settings().tab_size)
                .add_str(
                    checker
                        .locator()
                        .slice(TextRange::new(line_start, stmt.start())),
                )
                .add_str(&contents)
                > checker.settings().pycodestyle.max_line_length
            {
                return;
            }

            let mut diagnostic = checker.report_diagnostic(
                ReimplementedBuiltin {
                    replacement: contents.clone(),
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
    test: Expr<'a>,
    /// The target of the loop.
    target: Expr<'a>,
    /// The iterator of the loop.
    iter: Expr<'a>,
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
    stmt: Stmt<'a>,
}

fn match_loop(stmt: Stmt<'_>) -> Option<Loop<'_>> {
    let StmtKind::For(for_stmt) = stmt.classify() else {
        return None;
    };
    let body = for_stmt.body();
    let target = for_stmt.target();
    let iter = for_stmt.iter();

    // The loop itself should contain a single `if` statement, with a single `return` statement in
    // the body.
    let Some(
        [
            StmtFields::If(StmtIfFields {
                body: nested_body,
                test: nested_test,
                elif_else_clauses: nested_elif_else_clauses,
                ..
            }),
        ],
    ) = body.as_array().map(|items| items.map(Stmt::fields))
    else {
        return None;
    };
    if !nested_elif_else_clauses.is_empty() {
        return None;
    }
    let [stmt] = nested_body.as_array()?;
    let StmtFields::Return(StmtReturnFields {
        value: Some(value), ..
    }) = stmt.fields()
    else {
        return None;
    };
    let ExprKind::BooleanLiteral(boolean_literal) = value.classify() else {
        return None;
    };
    let value = boolean_literal.value();

    Some(Loop {
        return_value: value,
        test: nested_test,
        target,
        iter,
    })
}

/// If a `for` loop contains an `else` with a single boolean `return`, return the [`Terminal`]
/// representing that `return`.
///
/// For example, matches the `return` in:
/// ```python
/// for x in y:
///     if x == 0:
///         return True
/// return False
/// ```
fn match_else_return(stmt: Stmt<'_>) -> Option<Terminal<'_>> {
    let StmtKind::For(for_stmt) = stmt.classify() else {
        return None;
    };
    let orelse = for_stmt.orelse();

    // The `else` block has to contain a single `return True` or `return False`.
    let Some(
        [
            StmtFields::Return(StmtReturnFields {
                value: Some(next_value),
                ..
            }),
        ],
    ) = orelse.as_array().map(|items| items.map(Stmt::fields))
    else {
        return None;
    };
    let ExprKind::BooleanLiteral(boolean_literal) = next_value.classify() else {
        return None;
    };
    let next_value = boolean_literal.value();

    Some(Terminal {
        return_value: next_value,
        stmt,
    })
}

/// If a `for` loop is followed by a boolean `return`, return the [`Terminal`] representing that
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
fn match_sibling_return<'a>(stmt: Stmt<'a>, sibling: Stmt<'a>) -> Option<Terminal<'a>> {
    let StmtKind::For(for_stmt) = stmt.classify() else {
        return None;
    };
    let orelse = for_stmt.orelse();

    // The loop itself shouldn't have an `else` block.
    if !orelse.is_empty() {
        return None;
    }

    // The next statement has to be a `return True` or `return False`.
    let StmtFields::Return(StmtReturnFields {
        value: Some(next_value),
        range: _,
        node: _,
    }) = sibling.fields()
    else {
        return None;
    };
    let ExprKind::BooleanLiteral(boolean_literal) = next_value.classify() else {
        return None;
    };
    let next_value = boolean_literal.value();

    Some(Terminal {
        return_value: next_value,
        stmt: sibling,
    })
}

/// Generate a return statement for an `any` or `all` builtin comprehension.
fn return_stmt(
    builder: &mut Builder,
    builtin: &str,
    test: BuildId<ruff_python_ast::kind::Expr>,
    target: Expr<'_>,
    iter: Expr<'_>,
    generator: Generator,
) -> String {
    let range = TextRange::default();
    let comprehension = build::Comprehension {
        range,
        target: builder.copy_node(target),
        iter: builder.copy_node(iter),
        ifs: ListId::default(),
        is_async: false,
    };
    let comprehension = builder.add(comprehension);
    let generators = builder.list([comprehension]);
    let expression = builder.add(build::ExprGenerator {
        range,
        elt: test,
        generators,
        parenthesized: false,
    });
    let argument: BuildId<ruff_python_ast::kind::Expr> = expression.into();
    let args = builder.list([argument]);
    let value = syntax::call_name(builder, builtin, args);
    let statement = builder.add(build::StmtReturn {
        range,
        value: Some(value),
    });
    generator.stmt(builder.get(statement).into())
}
/// Return `true` if the [`Expr`] contains an `await`, `yield`, or `yield from` expression.
fn contains_yield_like(expr: Expr<'_>) -> bool {
    any_over_expr(expr, ruff_python_ast::Expr::is_await_expr)
        || any_over_expr(expr, ruff_python_ast::Expr::is_yield_expr)
        || any_over_expr(expr, ruff_python_ast::Expr::is_yield_from_expr)
}
