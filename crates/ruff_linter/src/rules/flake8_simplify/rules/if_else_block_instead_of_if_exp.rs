use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, BoolOp, ElifElseClause, Expr, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::fits;

/// ## What it does
/// Check for `if`-`else`-blocks that can be replaced with a ternary operator.
/// Moreover, in [preview], check if these ternary expressions can be
/// further simplified to binary expressions.
///
/// ## Why is this bad?
/// `if`-`else`-blocks that assign a value to a variable in both branches can
/// be expressed more concisely by using a ternary or binary operator.
///
/// ## Example
///
/// ```python
/// if foo:
///     bar = x
/// else:
///     bar = y
/// ```
///
/// Use instead:
/// ```python
/// bar = x if foo else y
/// ```
///
/// Or, in [preview]:
///
/// ```python
/// if cond:
///     z = cond
/// else:
///     z = other_cond
/// ```
///
/// Use instead:
///
/// ```python
/// z = cond or other_cond
/// ```
///
/// ## Known issues
/// This is an opinionated style rule that may not always be to everyone's
/// taste, especially for code that makes use of complex `if` conditions.
/// Ternary operators can also make it harder to measure [code coverage]
/// with tools that use line profiling.
///
/// ## References
/// - [Python documentation: Conditional expressions](https://docs.python.org/3/reference/expressions.html#conditional-expressions)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
/// [code coverage]: https://github.com/nedbat/coveragepy/issues/509
#[derive(ViolationMetadata)]
pub(crate) struct IfElseBlockInsteadOfIfExp {
    /// The ternary or binary expression to replace the `if`-`else`-block.
    contents: String,
    /// Whether to use a binary or ternary assignment.
    kind: AssignmentKind,
}

impl Violation for IfElseBlockInsteadOfIfExp {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfIfExp { contents, kind } = self;
        match kind {
            AssignmentKind::Ternary => {
                format!("Use ternary operator `{contents}` instead of `if`-`else`-block")
            }
            AssignmentKind::Binary => {
                format!("Use binary operator `{contents}` instead of `if`-`else`-block")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let IfElseBlockInsteadOfIfExp { contents, .. } = self;
        Some(format!("Replace `if`-`else`-block with `{contents}`"))
    }
}

/// SIM108
pub(crate) fn if_else_block_instead_of_if_exp(checker: &Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    } = stmt_if;

    // `test: None` to only match an `else` clause
    let [ElifElseClause {
        body: else_body,
        test: None,
        ..
    }] = elif_else_clauses.as_slice()
    else {
        return;
    };
    let [Stmt::Assign(ast::StmtAssign {
        targets: body_targets,
        value: body_value,
        ..
    })] = body.as_slice()
    else {
        return;
    };
    let [Stmt::Assign(ast::StmtAssign {
        targets: else_targets,
        value: else_value,
        ..
    })] = else_body.as_slice()
    else {
        return;
    };
    let ([body_target], [else_target]) = (body_targets.as_slice(), else_targets.as_slice()) else {
        return;
    };
    let Expr::Name(ast::ExprName { id: body_id, .. }) = body_target else {
        return;
    };
    let Expr::Name(ast::ExprName { id: else_id, .. }) = else_target else {
        return;
    };
    if body_id != else_id {
        return;
    }

    // Avoid suggesting ternary for `if (yield ...)`-style checks.
    // TODO(charlie): Fix precedence handling for yields in generator.
    if matches!(
        body_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }
    if matches!(
        else_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    // In most cases we should now suggest a ternary operator,
    // but there are three edge cases where a binary operator
    // is more appropriate.
    //
    // For the reader's convenience, here is how
    // the notation translates to the if-else block:
    //
    // ```python
    // if test:
    //     target_var = body_value
    // else:
    //     target_var = else_value
    // ```
    //
    // The match statement below implements the following
    // logic:
    //     - If `test == body_value` and preview enabled, replace with `target_var = test or else_value`
    //     - If `test == not body_value` and preview enabled, replace with `target_var = body_value and else_value`
    //     - If `not test == body_value` and preview enabled, replace with `target_var = body_value and else_value`
    //     - Otherwise, replace with `target_var = body_value if test else else_value`
    let (contents, assignment_kind) =
        match (checker.settings.preview.is_enabled(), test, body_value) {
            (true, test_node, body_node)
                if ComparableExpr::from(test_node) == ComparableExpr::from(body_node)
                    && !contains_effect(test_node, |id| {
                        checker.semantic().has_builtin_binding(id)
                    }) =>
            {
                let target_var = &body_target;
                let binary = assignment_binary_or(target_var, body_value, else_value);
                (checker.generator().stmt(&binary), AssignmentKind::Binary)
            }
            (true, test_node, body_node)
                if (test_node.as_unary_op_expr().is_some_and(|op_expr| {
                    op_expr.op.is_not()
                        && ComparableExpr::from(&op_expr.operand) == ComparableExpr::from(body_node)
                }) || body_node.as_unary_op_expr().is_some_and(|op_expr| {
                    op_expr.op.is_not()
                        && ComparableExpr::from(&op_expr.operand) == ComparableExpr::from(test_node)
                })) && !contains_effect(test_node, |id| {
                    checker.semantic().has_builtin_binding(id)
                }) =>
            {
                let target_var = &body_target;
                let binary = assignment_binary_and(target_var, body_value, else_value);
                (checker.generator().stmt(&binary), AssignmentKind::Binary)
            }
            _ => {
                let target_var = &body_target;
                let ternary = assignment_ternary(target_var, body_value, test, else_value);
                (checker.generator().stmt(&ternary), AssignmentKind::Ternary)
            }
        };

    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &contents,
        stmt_if.into(),
        checker.locator(),
        checker.settings.pycodestyle.max_line_length,
        checker.settings.tab_size,
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfIfExp {
            contents: contents.clone(),
            kind: assignment_kind,
        },
        stmt_if.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(stmt_if, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_if.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentKind {
    Binary,
    Ternary,
}

fn assignment_ternary(
    target_var: &Expr,
    body_value: &Expr,
    test: &Expr,
    orelse_value: &Expr,
) -> Stmt {
    let node = ast::ExprIf {
        test: Box::new(test.clone()),
        body: Box::new(body_value.clone()),
        orelse: Box::new(orelse_value.clone()),
        range: TextRange::default(),
    };
    let node1 = ast::StmtAssign {
        targets: vec![target_var.clone()],
        value: Box::new(node.into()),
        range: TextRange::default(),
    };
    node1.into()
}

fn assignment_binary_and(target_var: &Expr, left_value: &Expr, right_value: &Expr) -> Stmt {
    let node = ast::ExprBoolOp {
        op: BoolOp::And,
        values: vec![left_value.clone(), right_value.clone()],
        range: TextRange::default(),
    };
    let node1 = ast::StmtAssign {
        targets: vec![target_var.clone()],
        value: Box::new(node.into()),
        range: TextRange::default(),
    };
    node1.into()
}

fn assignment_binary_or(target_var: &Expr, left_value: &Expr, right_value: &Expr) -> Stmt {
    (ast::StmtAssign {
        range: TextRange::default(),
        targets: vec![target_var.clone()],
        value: Box::new(
            (ast::ExprBoolOp {
                range: TextRange::default(),
                op: BoolOp::Or,
                values: vec![left_value.clone(), right_value.clone()],
            })
            .into(),
        ),
    })
    .into()
}
