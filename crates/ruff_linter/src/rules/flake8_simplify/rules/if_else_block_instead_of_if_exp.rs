use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::flat::{self, fields};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, BoolOp, Expr, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::fits;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Check for `if`-`else`-blocks that can be replaced with a ternary
/// or binary operator.
///
/// The lint is suppressed if the suggested replacement would exceed
/// the maximum line length configured in [pycodestyle.max-line-length].
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
/// Or:
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
/// ## Options
///
/// - `lint.pycodestyle.max-line-length`
/// - `indent-width`
///
/// ## References
/// - [Python documentation: Conditional expressions](https://docs.python.org/3/reference/expressions.html#conditional-expressions)
///
/// [code coverage]: https://github.com/nedbat/coveragepy/issues/509
/// [pycodestyle.max-line-length]: https://docs.astral.sh/ruff/settings/#lint_pycodestyle_max-line-length
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.213", category = Category::Pedantic)]
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
pub(crate) fn if_else_block_instead_of_if_exp(
    checker: &Checker,
    stmt_if: flat::View<'_, ast::StmtIf>,
) {
    let test = stmt_if.test();
    let body = stmt_if.body();
    let elif_else_clauses = stmt_if.elif_else_clauses();

    // `test: None` to only match an `else` clause
    let Some([elif_else_clause]) = elif_else_clauses.as_array() else {
        return;
    };
    let fields::ElifElseClause {
        body: else_body,
        test: None,
        ..
    } = elif_else_clause.fields()
    else {
        return;
    };
    let Some(
        [
            fields::Stmt::Assign(fields::StmtAssign {
                targets: body_targets,
                value: body_value,
                ..
            }),
        ],
    ) = body
        .as_array()
        .map(|items| items.map(flat::View::<Stmt>::fields))
    else {
        return;
    };
    let Some(
        [
            fields::Stmt::Assign(fields::StmtAssign {
                targets: else_targets,
                value: else_value,
                ..
            }),
        ],
    ) = else_body.as_array().map(|[stmt]| [stmt.fields()])
    else {
        return;
    };
    let (Some([body_target]), Some([else_target])) =
        (body_targets.as_array(), else_targets.as_array())
    else {
        return;
    };
    let flat::ExprKind::Name(name) = body_target.classify() else {
        return;
    };
    let body_id = name.id();
    let flat::ExprKind::Name(name) = else_target.classify() else {
        return;
    };
    let else_id = name.id();
    if body_id != else_id {
        return;
    }

    // Avoid suggesting ternary for `if (yield ...)`-style checks.
    // TODO(charlie): Fix precedence handling for yields in generator.
    if matches!(
        body_value.classify(),
        flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_) | flat::ExprKind::Await(_)
    ) {
        return;
    }
    if matches!(
        else_value.classify(),
        flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_) | flat::ExprKind::Await(_)
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
    //     - If `test == body_value`, replace with `target_var = test or else_value`
    //     - If `test == not body_value`, replace with `target_var = body_value and else_value`
    //     - If `not test == body_value`, replace with `target_var = body_value and else_value`
    //     - Otherwise, replace with `target_var = body_value if test else else_value`
    let (contents, assignment_kind) = match (test, body_value) {
        (test_node, body_node)
            if ComparableExpr::from(test_node) == ComparableExpr::from(body_node)
                && !contains_effect(test_node, |id| checker.semantic().has_builtin_binding(id)) =>
        {
            let target_var = body_target;
            let binary = assignment_binary_or(target_var, body_value, else_value);
            (checker.generator().stmt(&binary), AssignmentKind::Binary)
        }
        (test_node, body_node)
            if (test_node.as_unary_op_expr().is_some_and(|op_expr| {
                op_expr.op().is_not()
                    && ComparableExpr::from(op_expr.operand()) == ComparableExpr::from(body_node)
            }) || body_node.as_unary_op_expr().is_some_and(|op_expr| {
                op_expr.op().is_not()
                    && ComparableExpr::from(op_expr.operand()) == ComparableExpr::from(test_node)
            })) && !contains_effect(test_node, |id| {
                checker.semantic().has_builtin_binding(id)
            }) =>
        {
            let target_var = body_target;
            let binary = assignment_binary_and(target_var, body_value, else_value);
            (checker.generator().stmt(&binary), AssignmentKind::Binary)
        }
        _ => {
            let target_var = body_target;
            let ternary = assignment_ternary(target_var, body_value, test, else_value);
            (checker.generator().stmt(&ternary), AssignmentKind::Ternary)
        }
    };

    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &contents,
        stmt_if.into(),
        checker.locator(),
        checker.settings().pycodestyle.max_line_length,
        checker.settings().tab_size,
    ) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        IfElseBlockInsteadOfIfExp {
            contents: contents.clone(),
            kind: assignment_kind,
        },
        stmt_if.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(&stmt_if, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_if.range(),
        )));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentKind {
    Binary,
    Ternary,
}

fn assignment_ternary(
    target_var: flat::View<'_, Expr>,
    body_value: flat::View<'_, Expr>,
    test: flat::View<'_, Expr>,
    orelse_value: flat::View<'_, Expr>,
) -> Stmt {
    let node = ast::ExprIf {
        test: Box::new(test.to_owned_node()),
        body: Box::new(body_value.to_owned_node()),
        orelse: Box::new(orelse_value.to_owned_node()),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    let node1 = ast::StmtAssign {
        targets: vec![target_var.to_owned_node()],
        value: Box::new(node.into()),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    node1.into()
}

fn assignment_binary_and(
    target_var: flat::View<'_, Expr>,
    left_value: flat::View<'_, Expr>,
    right_value: flat::View<'_, Expr>,
) -> Stmt {
    let node = ast::ExprBoolOp {
        op: BoolOp::And,
        values: vec![left_value.to_owned_node(), right_value.to_owned_node()],
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    let node1 = ast::StmtAssign {
        targets: vec![target_var.to_owned_node()],
        value: Box::new(node.into()),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    node1.into()
}

fn assignment_binary_or(
    target_var: flat::View<'_, Expr>,
    left_value: flat::View<'_, Expr>,
    right_value: flat::View<'_, Expr>,
) -> Stmt {
    (ast::StmtAssign {
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
        targets: vec![target_var.to_owned_node()],
        value: Box::new(
            (ast::ExprBoolOp {
                range: TextRange::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
                op: BoolOp::Or,
                values: vec![left_value.to_owned_node(), right_value.to_owned_node()],
            })
            .into(),
        ),
    })
    .into()
}
