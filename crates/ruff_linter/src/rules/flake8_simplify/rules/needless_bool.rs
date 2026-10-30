use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::traversal;
use ruff_python_ast::flat::{self, fields};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
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
/// This fix is marked as unsafe because it may change the program’s behavior if the condition does not
/// return a proper Boolean. While the fix will try to wrap non-boolean values in a call to bool,
/// custom implementations of comparison functions like `__eq__` can avoid the bool call and still
/// lead to altered behavior.
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.214", category = Category::Complexity)]
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
pub(crate) fn needless_bool(checker: &Checker, stmt: flat::View<'_, Stmt>) {
    let flat::StmtKind::If(stmt_if) = stmt.classify() else {
        return;
    };
    let if_test = stmt_if.test();
    let if_body = stmt_if.body();
    let elif_else_clauses = stmt_if.elif_else_clauses();

    // Extract an `if` or `elif` (that returns) followed by an else (that returns the same value)
    let (if_test, if_body, else_body, range) = match elif_else_clauses {
        // if-else case:
        // ```python
        // if x > 0:
        //     return True
        // else:
        //     return False
        // ```
        items
            if let Some([elif_else_clause]) = items.as_array()
                && let fields::ElifElseClause {
                    body: else_body,
                    test: None,
                    ..
                } = elif_else_clause.fields() =>
        {
            (
                if_test,
                if_body,
                is_one_line_return_bool(else_body),
                stmt_if.range(),
            )
        }
        // elif-else case
        // ```python
        // if x > 0:
        //     return True
        // elif x < 0:
        //     return False
        // ```
        items
            if let Some((_, [elif_else_clause, else_clause])) = items.split_last_chunk()
                && let fields::ElifElseClause {
                    body: elif_body,
                    test: Some(elif_test),
                    range: elif_range,
                    ..
                } = elif_else_clause.fields()
                && let fields::ElifElseClause {
                    body: else_body,
                    test: None,
                    range: else_range,
                    ..
                } = else_clause.fields() =>
        {
            (
                elif_test,
                elif_body,
                is_one_line_return_bool(else_body),
                TextRange::new(elif_range.start(), else_range.end()),
            )
        }
        // if-implicit-else case:
        // ```python
        // if x > 0:
        //     return True
        // return False
        // ```
        items if items.is_empty() => {
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
                if_test,
                if_body,
                return_bool(next_stmt),
                TextRange::new(stmt_if.start(), next_stmt.end()),
            )
        }
        _ => return,
    };

    // Both branches must be one-liners that return a boolean.
    let (Some(if_return), Some(else_return)) = (is_one_line_return_bool(if_body), else_body) else {
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

    // Generate the replacement condition.
    let condition = if checker
        .comment_ranges()
        .has_comments(&range, checker.source())
    {
        None
    } else {
        // If the return values are inverted, wrap the condition in a `not`.
        if inverted {
            match if_test.classify() {
                flat::ExprKind::UnaryOp(expr_unary_op)
                    if expr_unary_op.op() == ast::UnaryOp::Not =>
                {
                    let operand = expr_unary_op.operand();
                    Some(operand.to_owned_node())
                }

                flat::ExprKind::Compare(expr_compare)
                    if let Some(
                        [
                            op @ (ast::CmpOp::Eq
                            | ast::CmpOp::NotEq
                            | ast::CmpOp::In
                            | ast::CmpOp::NotIn
                            | ast::CmpOp::Is
                            | ast::CmpOp::IsNot),
                        ],
                    ) = expr_compare.ops().as_array() =>
                {
                    let operands = expr_compare.operands();

                    Some(Expr::Compare(ast::ExprCompare {
                        ops: [op.negate()].into(),
                        operands: operands.iter().map(flat::View::to_owned_node).collect(),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::NodeIndex::NONE,
                    }))
                }

                _ => Some(Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand: Box::new(if_test.to_owned_node()),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::NodeIndex::NONE,
                })),
            }
        } else if if_test.is_compare_expr() {
            // If the condition is a comparison, we can replace it with the condition, since we
            // know it's a boolean.
            Some(if_test.to_owned_node())
        } else if checker.semantic().has_builtin_binding("bool") {
            // Otherwise, we need to wrap the condition in a call to `bool`.
            let func_node = ast::ExprName {
                id: Name::new_static("bool"),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
            };
            let call_node = ast::ExprCall {
                func: Box::new(func_node.into()),
                arguments: Arguments {
                    args: [if_test].map(flat::View::to_owned_node).into(),
                    keywords: std::iter::empty().collect(),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::NodeIndex::NONE,
                },
                range_start: ruff_text_size::TextSize::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
            };
            Some(Expr::Call(call_node))
        } else {
            None
        }
    };

    // Generate the replacement `return` statement.
    let replacement = condition.as_ref().map(|expr| {
        Stmt::Return(ast::StmtReturn {
            value: Some(Box::new(expr.clone())),
            range: TextRange::default(),
            node_index: ruff_python_ast::NodeIndex::NONE,
        })
    });

    // Generate source code.
    let replacement = replacement.map(|stmt| checker.generator().stmt(&stmt));
    let condition = condition.map(|expr| checker.generator().expr(&expr));

    let mut diagnostic = checker.report_diagnostic(
        NeedlessBool {
            condition: condition.map(SourceCodeSnippet::new),
            negate: inverted,
        },
        range,
    );
    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            replacement,
            range,
        )));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bool {
    True,
    False,
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value { Bool::True } else { Bool::False }
    }
}

fn is_one_line_return_bool(stmts: flat::Suite<'_>) -> Option<Bool> {
    let [stmt] = stmts.as_array()?;
    return_bool(stmt)
}

fn return_bool(stmt: flat::View<'_, Stmt>) -> Option<Bool> {
    let flat::StmtKind::Return(return_stmt) = stmt.classify() else {
        return None;
    };
    let value = return_stmt.value();
    let Some(fields::Expr::BooleanLiteral(fields::ExprBooleanLiteral { value, .. })) =
        value.map(flat::View::<Expr>::fields)
    else {
        return None;
    };
    Some(value.into())
}
