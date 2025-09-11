use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal;
use ruff_python_ast::{self as ast, Arguments, ElifElseClause, Expr, ExprContext, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `if` statements that can be replaced with `bool`.
///
/// ## Why is this bad?
/// `if` statements that return `True` for a truthy condition and `False` for
/// a falsy condition can be replaced with boolean casts.
///
/// ## Example
/// Given:
/// ```python
/// def foo(x: int) -> bool:
///     if x > 0:
///         return True
///     else:
///         return False
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int) -> bool:
///     return x > 0
/// ```
///
/// Or, given:
/// ```python
/// def foo(x: int) -> bool:
///     if x > 0:
///         return True
///     return False
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int) -> bool:
///     return x > 0
/// ```
///
/// ## Fix safety
///
/// This rule provides safe fixes when the replacement expression is guaranteed to evaluate
/// to a real boolean value – for example, a logical negation (`not`), an identity or
/// membership comparison (`is`, `is not`, `in`, `not in`), or a call to the builtin
/// `bool(...)`.
///
/// In other cases, the fix is marked as unsafe because it can change runtime behavior. In
/// particular, equality and inequality comparisons (`==`, `!=`) may be overloaded to return
/// non-boolean values. When `bool` is shadowed and the expression is not guaranteed to be
/// boolean, no fix is offered.
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
pub(crate) struct NeedlessBool {
    condition: Option<SourceCodeSnippet>,
    negate: bool,
}

impl Violation for NeedlessBool {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NeedlessBool { condition, negate } = self;
        if let Some(condition) = condition.as_ref().and_then(SourceCodeSnippet::full_display) {
            format!("Return the condition `{condition}` directly")
        } else if *negate {
            "Return the negated condition directly".to_string()
        } else {
            "Return the condition directly".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let NeedlessBool { condition, .. } = self;
        Some(
            if let Some(condition) = condition.as_ref().and_then(SourceCodeSnippet::full_display) {
                format!("Replace with `return {condition}`")
            } else {
                "Inline condition".to_string()
            },
        )
    }
}

/// SIM103
pub(crate) fn needless_bool(checker: &Checker, stmt: &Stmt) {
    let Stmt::If(stmt_if) = stmt else { return };
    let ast::StmtIf {
        test: if_test,
        body: if_body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // Extract an `if` or `elif` (that returns) followed by an else (that returns the same value)
    let (if_test, if_body, else_body, range) = match elif_else_clauses.as_slice() {
        // if-else case:
        // ```python
        // if x > 0:
        //     return True
        // else:
        //     return False
        // ```
        [
            ElifElseClause {
                body: else_body,
                test: None,
                ..
            },
        ] => (
            if_test.as_ref(),
            if_body,
            else_body.as_slice(),
            stmt_if.range(),
        ),
        // elif-else case
        // ```python
        // if x > 0:
        //     return True
        // elif x < 0:
        //     return False
        // ```
        [
            ..,
            ElifElseClause {
                body: elif_body,
                test: Some(elif_test),
                range: elif_range,
                node_index: _,
            },
            ElifElseClause {
                body: else_body,
                test: None,
                range: else_range,
                node_index: _,
            },
        ] => (
            elif_test,
            elif_body,
            else_body.as_slice(),
            TextRange::new(elif_range.start(), else_range.end()),
        ),
        // if-implicit-else case:
        // ```python
        // if x > 0:
        //     return True
        // return False
        // ```
        [] => {
            // Fetching the next sibling is expensive, so do some validation early.
            if is_one_line_return_bool(if_body).is_none() {
                return;
            }

            // Fetch the next sibling statement.
            let Some(next_stmt) = checker
                .semantic()
                .current_statement_parent()
                .and_then(|parent| traversal::suite(stmt, parent))
                .and_then(|suite| suite.next_sibling())
            else {
                return;
            };

            // If the next sibling is not a return statement, abort.
            if !next_stmt.is_return_stmt() {
                return;
            }

            (
                if_test.as_ref(),
                if_body,
                std::slice::from_ref(next_stmt),
                TextRange::new(stmt_if.start(), next_stmt.end()),
            )
        }
        _ => return,
    };

    // Both branches must be one-liners that return a boolean.
    let (Some(if_return), Some(else_return)) = (
        is_one_line_return_bool(if_body),
        is_one_line_return_bool(else_body),
    ) else {
        return;
    };

    // Determine whether the return values are inverted, as in:
    // ```python
    // if x > 0:
    //     return False
    // else:
    //     return True
    // ```
    let inverted = match (if_return, else_return) {
        (Bool::True, Bool::False) => false,
        (Bool::False, Bool::True) => true,
        // If the branches have the same condition, abort (although the code could be
        // simplified).
        _ => return,
    };

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    // Build replacement condition and decide safety together.
    let (condition, applicability) = if checker
        .comment_ranges()
        .has_comments(&range, checker.source())
    {
        (None, Applicability::Unsafe)
    } else {
        build_replacement_and_safety(
            if_test,
            inverted,
            checker.semantic().has_builtin_binding("bool"),
        )
    };

    // Generate the replacement `return` statement.
    let replacement = condition.as_ref().map(|expr| {
        Stmt::Return(ast::StmtReturn {
            value: Some(Box::new(expr.clone())),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })
    });

    // Generate source code.
    let replacement = replacement
        .as_ref()
        .map(|stmt| checker.generator().stmt(stmt));
    let condition_code = condition
        .as_ref()
        .map(|expr| checker.generator().expr(expr));

    let mut diagnostic = checker.report_diagnostic(
        NeedlessBool {
            condition: condition_code.map(SourceCodeSnippet::new),
            negate: inverted,
        },
        range,
    );
    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(replacement, range),
            applicability,
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bool {
    True,
    False,
}

/// Build the replacement expression for SIM103 and determine whether applying it is safe.
///
/// Safety rules:
/// - Safe if the replacement is guaranteed boolean:
///   - a negation (`not <expr>`),
///   - an identity/membership comparison (`is`, `is not`, `in`, `not in`), or
///   - a call to the builtin `bool(<expr>)` (only when `bool` is not shadowed).
/// - Unsafe for equality comparisons (`==`, `!=`), because user types may overload them to return
///   non-boolean values.
/// - When `bool` is shadowed and the expression is not guaranteed boolean, no fix is provided.
fn build_replacement_and_safety(
    if_test: &Expr,
    inverted: bool,
    has_builtin_bool: bool,
) -> (Option<Expr>, Applicability) {
    fn is_identity_or_membership_ops(ops: &[ast::CmpOp]) -> bool {
        ops.iter().all(|op| {
            matches!(
                op,
                ast::CmpOp::In | ast::CmpOp::NotIn | ast::CmpOp::Is | ast::CmpOp::IsNot
            )
        })
    }

    fn is_eq_neq_ops(ops: &[ast::CmpOp]) -> bool {
        ops.iter()
            .all(|op| matches!(op, ast::CmpOp::Eq | ast::CmpOp::NotEq))
    }

    match (inverted, if_test) {
        // Replacement becomes the operand; safe only if guaranteed-boolean.
        (
            true,
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }),
        ) => match operand.as_ref() {
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                ..
            }) => (Some((**operand).clone()), Applicability::Safe),
            Expr::Compare(ast::ExprCompare { ops, .. }) => {
                let app = if is_identity_or_membership_ops(ops.as_ref()) {
                    Applicability::Safe
                } else {
                    Applicability::Unsafe
                };
                (Some((**operand).clone()), app)
            }
            _ => (Some((**operand).clone()), Applicability::Unsafe),
        },

        // Replacement becomes a negated comparison: safe only for identity/membership. For
        // other comparisons, the replacement will be `not <expr>` which is a bool, except for
        // `==`/`!=` which can be overloaded to return non-bool.
        (
            true,
            Expr::Compare(ast::ExprCompare {
                ops,
                left,
                comparators,
                ..
            }),
        ) => match ops.as_ref() {
            [
                ast::CmpOp::Eq
                | ast::CmpOp::NotEq
                | ast::CmpOp::In
                | ast::CmpOp::NotIn
                | ast::CmpOp::Is
                | ast::CmpOp::IsNot,
            ] => {
                let ([op], [right]) = (ops.as_ref(), comparators.as_ref()) else {
                    unreachable!("Single comparison with multiple comparators");
                };
                let replacement = Expr::Compare(ast::ExprCompare {
                    ops: Box::new([op.negate()]),
                    left: left.clone(),
                    comparators: Box::new([right.clone()]),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                });
                (
                    Some(replacement),
                    if is_identity_or_membership_ops(ops.as_ref()) && !is_eq_neq_ops(ops.as_ref()) {
                        Applicability::Safe
                    } else {
                        Applicability::Unsafe
                    },
                )
            }
            _ => {
                let replacement = Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand: Box::new(if_test.clone()),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                });
                (Some(replacement), Applicability::Safe)
            }
        },

        // Replacement becomes `not <expr>` which is always a bool.
        (true, _) => (
            Some(Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand: Box::new(if_test.clone()),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            })),
            Applicability::Safe,
        ),

        // Non-inverted: direct compare is safe only for identity/membership; otherwise
        // we rely on wrapping with `bool(...)` if available.
        (false, Expr::Compare(ast::ExprCompare { ops, .. })) => (
            Some(if_test.clone()),
            if is_identity_or_membership_ops(ops.as_ref()) {
                Applicability::Safe
            } else {
                Applicability::Unsafe
            },
        ),
        (false, _) if has_builtin_bool => {
            let func_node = ast::ExprName {
                id: Name::new_static("bool"),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            };
            let call_node = ast::ExprCall {
                func: Box::new(func_node.into()),
                arguments: Arguments {
                    args: Box::from([if_test.clone()]),
                    keywords: Box::from([]),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                },
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            };
            (Some(Expr::Call(call_node)), Applicability::Safe)
        }
        (false, _) => (None, Applicability::Unsafe),
    }
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value { Bool::True } else { Bool::False }
    }
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> Option<Bool> {
    let [stmt] = stmts else {
        return None;
    };
    let Stmt::Return(ast::StmtReturn {
        value,
        range: _,
        node_index: _,
    }) = stmt
    else {
        return None;
    };
    let Some(Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. })) = value.as_deref() else {
        return None;
    };
    Some((*value).into())
}
