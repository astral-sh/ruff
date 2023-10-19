use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Constant, ElifElseClause, Expr, ExprContext, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `if` statements that can be replaced with `bool`.
///
/// ## Why is this bad?
/// `if` statements that return `True` for a truthy condition and `False` for
/// a falsey condition can be replaced with boolean casts.
///
/// ## Example
/// ```python
/// if foo:
///     return True
/// else:
///     return False
/// ```
///
/// Use instead:
/// ```python
/// return bool(foo)
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct NeedlessBool {
    condition: String,
}

impl Violation for NeedlessBool {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NeedlessBool { condition } = self;
        format!("Return the condition `{condition}` directly")
    }

    fn fix_title(&self) -> Option<String> {
        let NeedlessBool { condition } = self;
        Some(format!("Replace with `return {condition}`"))
    }
}

/// SIM103
pub(crate) fn needless_bool(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test: if_test,
        body: if_body,
        elif_else_clauses,
        range: _,
    } = stmt_if;

    // Extract an `if` or `elif` (that returns) followed by an else (that returns the same value)
    let (if_test, if_body, else_body, range) = match elif_else_clauses.as_slice() {
        // if-else case
        [ElifElseClause {
            body: else_body,
            test: None,
            ..
        }] => (if_test.as_ref(), if_body, else_body, stmt_if.range()),
        // elif-else case
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
            else_body,
            TextRange::new(elif_range.start(), else_range.end()),
        ),
        _ => return,
    };

    let (Some(if_return), Some(else_return)) = (
        is_one_line_return_bool(if_body),
        is_one_line_return_bool(else_body),
    ) else {
        return;
    };

    // If the branches have the same condition, abort (although the code could be
    // simplified).
    if if_return == else_return {
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

    let condition = checker.generator().expr(if_test);
    let mut diagnostic = Diagnostic::new(NeedlessBool { condition }, range);
    if matches!(if_return, Bool::True)
        && matches!(else_return, Bool::False)
        && !checker.indexer().has_comments(&range, checker.locator())
        && (if_test.is_compare_expr() || checker.semantic().is_builtin("bool"))
    {
        if if_test.is_compare_expr() {
            // If the condition is a comparison, we can replace it with the condition.
            let node = ast::StmtReturn {
                value: Some(Box::new(if_test.clone())),
                range: TextRange::default(),
            };
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().stmt(&node.into()),
                range,
            )));
        } else {
            // Otherwise, we need to wrap the condition in a call to `bool`. (We've already
            // verified, above, that `bool` is a builtin.)
            let node = ast::ExprName {
                id: "bool".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            let node1 = ast::ExprCall {
                func: Box::new(node.into()),
                arguments: Arguments {
                    args: vec![if_test.clone()],
                    keywords: vec![],
                    range: TextRange::default(),
                },
                range: TextRange::default(),
            };
            let node2 = ast::StmtReturn {
                value: Some(Box::new(node1.into())),
                range: TextRange::default(),
            };
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().stmt(&node2.into()),
                range,
            )));
        };
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
    let Some(Expr::Constant(ast::ExprConstant { value, .. })) = value.as_deref() else {
        return None;
    };
    let Constant::Bool(value) = value else {
        return None;
    };
    Some((*value).into())
}
