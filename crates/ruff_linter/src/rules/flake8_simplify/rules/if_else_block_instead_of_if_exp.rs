use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, helpers, BoolOp, ElifElseClause, Expr, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_python_semantic::SemanticModel;
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
pub(crate) fn if_else_block_instead_of_if_exp(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let semantic = checker.semantic();
    let in_preview = checker.settings.preview.is_enabled();

    let Some((test, body_assignment, else_assignment)) = test_and_assignments(stmt_if) else {
        return;
    };

    if body_assignment.id != else_assignment.id {
        return;
    }

    let (annotation, None) = (body_assignment.annotation, else_assignment.annotation) else {
        return;
    };

    if annotation.is_some() && !in_preview {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, semantic) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, semantic) {
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
    let (contents, assignment_kind) = match (in_preview, test, body_assignment.value) {
        (true, test_node, body_node)
            if ComparableExpr::from(test_node) == ComparableExpr::from(body_node)
                && !contains_effect(test_node, semantic) =>
        {
            let target_var = &body_assignment.target;
            let binary = assignment_binary_or(
                target_var,
                annotation,
                body_assignment.value,
                else_assignment.value,
            );

            (checker.generator().stmt(&binary), AssignmentKind::Binary)
        }

        (true, test_node, body_node)
            if (is_inverted_of(test_node, body_node) || is_inverted_of(body_node, test_node))
                && !contains_effect(test_node, semantic) =>
        {
            let target_var = &body_assignment.target;
            let binary = assignment_binary_and(
                target_var,
                annotation,
                body_assignment.value,
                else_assignment.value,
            );

            (checker.generator().stmt(&binary), AssignmentKind::Binary)
        }

        _ => {
            let target_var = &body_assignment.target;
            let ternary = assignment_ternary(
                target_var,
                annotation,
                body_assignment.value,
                test,
                else_assignment.value,
            );

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
    checker.diagnostics.push(diagnostic);
}

fn test_and_assignments(
    stmt_if: &ast::StmtIf,
) -> Option<(&Expr, SimplifiableAssignment, SimplifiableAssignment)> {
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
        return None;
    };

    let body_assignment = SimplifiableAssignment::from_body(body)?;
    let else_assignment = SimplifiableAssignment::from_body(else_body)?;

    Some((test, body_assignment, else_assignment))
}

fn contains_effect(expr: &Expr, semantic: &SemanticModel) -> bool {
    helpers::contains_effect(expr, |id| semantic.has_builtin_binding(id))
}

fn is_inverted_of(first: &Expr, second: &Expr) -> bool {
    let Expr::UnaryOp(ast::ExprUnaryOp { op, operand, .. }) = first else {
        return false;
    };

    op.is_not() && ComparableExpr::from(operand) == ComparableExpr::from(second)
}

struct SimplifiableAssignment<'a> {
    target: &'a Expr,
    id: &'a ast::name::Name,
    annotation: Option<&'a Expr>,
    value: &'a Expr,
}

impl<'a> SimplifiableAssignment<'a> {
    fn from_body(body: &'a [Stmt]) -> Option<Self> {
        match body {
            [Stmt::Assign(assign)] => Self::from_assign(assign),
            [Stmt::AnnAssign(ann_assign)] => Self::from_ann_assign(ann_assign),
            _ => None,
        }
    }

    fn from_assign(stmt: &'a ast::StmtAssign) -> Option<Self> {
        let ast::StmtAssign { targets, value, .. } = stmt;

        let [target @ Expr::Name(ast::ExprName { id, .. })] = &targets[..] else {
            return None;
        };
        let annotation = None;

        if expr_is_yield_or_await(value) {
            return None;
        }

        Some(Self {
            target,
            id,
            annotation,
            value,
        })
    }

    fn from_ann_assign(stmt: &'a ast::StmtAnnAssign) -> Option<Self> {
        let ast::StmtAnnAssign {
            target,
            annotation,
            value,
            ..
        } = stmt;

        let target = target.as_ref();
        let Expr::Name(ast::ExprName { id, .. }) = target else {
            return None;
        };

        let annotation = Some(annotation.as_ref());

        let Some(value) = value else {
            return None;
        };
        let value = value.as_ref();

        if expr_is_yield_or_await(value) {
            return None;
        }

        Some(Self {
            target,
            id,
            annotation,
            value,
        })
    }
}

const fn expr_is_yield_or_await(expr: &Expr) -> bool {
    // Avoid suggesting ternary for `if (yield ...)`-style checks.
    // TODO(charlie): Fix precedence handling for yields in generator.
    matches!(expr, Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentKind {
    Binary,
    Ternary,
}

fn assignment_ternary(
    target_var: &Expr,
    annotation: Option<&Expr>,
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

    if let Some(annotation) = annotation {
        ast::StmtAnnAssign {
            target: Box::new(target_var.clone()),
            annotation: Box::new(annotation.clone()),
            value: Some(Box::new(node.into())),
            simple: true,
            range: TextRange::default(),
        }
        .into()
    } else {
        ast::StmtAssign {
            targets: vec![target_var.clone()],
            value: Box::new(node.into()),
            range: TextRange::default(),
        }
        .into()
    }
}

fn assignment_binary_and(
    target_var: &Expr,
    annotation: Option<&Expr>,
    left_value: &Expr,
    right_value: &Expr,
) -> Stmt {
    let node = ast::ExprBoolOp {
        op: BoolOp::And,
        values: vec![left_value.clone(), right_value.clone()],
        range: TextRange::default(),
    };

    if let Some(annotation) = annotation {
        ast::StmtAnnAssign {
            target: Box::new(target_var.clone()),
            annotation: Box::new(annotation.clone()),
            value: Some(Box::new(node.into())),
            simple: true,
            range: TextRange::default(),
        }
        .into()
    } else {
        ast::StmtAssign {
            targets: vec![target_var.clone()],
            value: Box::new(node.into()),
            range: TextRange::default(),
        }
        .into()
    }
}

fn assignment_binary_or(
    target_var: &Expr,
    annotation: Option<&Expr>,
    left_value: &Expr,
    right_value: &Expr,
) -> Stmt {
    let node = ast::ExprBoolOp {
        range: TextRange::default(),
        op: BoolOp::Or,
        values: vec![left_value.clone(), right_value.clone()],
    };

    if let Some(annotation) = annotation {
        ast::StmtAnnAssign {
            target: Box::new(target_var.clone()),
            annotation: Box::new(annotation.clone()),
            value: Some(Box::new(node.into())),
            simple: true,
            range: TextRange::default(),
        }
        .into()
    } else {
        ast::StmtAssign {
            targets: vec![target_var.clone()],
            value: Box::new(node.into()),
            range: TextRange::default(),
        }
        .into()
    }
}
