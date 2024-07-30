use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal;
use ruff_python_ast::{self as ast, Arguments, ElifElseClause, Expr, ExprContext, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for `if` statements that can be replaced with `bool`.
///
/// ## Why is this bad?
/// `if` statements that return `True` for a truthy condition and `False` for
/// a falsey condition can be replaced with boolean casts.
///
/// ## Example
/// Given:
/// ```python
/// if x > 0:
///     return True
/// else:
///     return False
/// ```
///
/// Use instead:
/// ```python
/// return x > 0
/// ```
///
/// Or, given:
/// ```python
/// if x > 0:
///     return True
/// return False
/// ```
///
/// Use instead:
/// ```python
/// return x > 0
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct NeedlessBool {
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
            format!("Return the negated condition directly")
        } else {
            format!("Return the condition directly")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let NeedlessBool { condition, .. } = self;

        if let Some(condition) = condition.as_ref().and_then(SourceCodeSnippet::full_display) {
            Some(format!("Replace with `return {condition}`"))
        } else {
            Some(format!("Inline condition"))
        }
    }
}

/// SIM103
pub(crate) fn needless_bool(checker: &mut Checker, stmt: &Stmt) {
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
        [ElifElseClause {
            body: else_body,
            test: None,
            ..
        }] => (
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
        [.., ElifElseClause {
            body: elif_body,
            test: Some(elif_test),
            range: elif_range,
        }, ElifElseClause {
            body: else_body,
            test: None,
            range: else_range,
        }] => (
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
                .and_then(|suite| traversal::next_sibling(stmt, suite))
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

    // Generate the replacement condition.
    let condition = if checker
        .comment_ranges()
        .has_comments(&range, checker.locator())
    {
        None
    } else {
        // If the return values are inverted, wrap the condition in a `not`.
        if inverted {
            if let Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) = if_test
            {
                Some((**operand).clone())
            } else {
                Some(Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand: Box::new(if_test.clone()),
                    range: TextRange::default(),
                }))
            }
        } else if if_test.is_compare_expr() {
            // If the condition is a comparison, we can replace it with the condition, since we
            // know it's a boolean.
            Some(if_test.clone())
        } else if checker.semantic().has_builtin_binding("bool") {
            // Otherwise, we need to wrap the condition in a call to `bool`.
            let func_node = ast::ExprName {
                id: Name::new_static("bool"),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            let call_node = ast::ExprCall {
                func: Box::new(func_node.into()),
                arguments: Arguments {
                    args: Box::from([if_test.clone()]),
                    keywords: Box::from([]),
                    range: TextRange::default(),
                },
                range: TextRange::default(),
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
        })
    });

    // Generate source code.
    let replacement = replacement
        .as_ref()
        .map(|stmt| checker.generator().stmt(stmt));
    let condition = condition
        .as_ref()
        .map(|expr| checker.generator().expr(expr));

    let mut diagnostic = Diagnostic::new(
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
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bool {
    True,
    False,
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value {
            Bool::True
        } else {
            Bool::False
        }
    }
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> Option<Bool> {
    let [stmt] = stmts else {
        return None;
    };
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = stmt else {
        return None;
    };
    let Some(Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. })) = value.as_deref() else {
        return None;
    };
    Some((*value).into())
}
