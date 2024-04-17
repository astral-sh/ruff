use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Stmt;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__index__` implementations that return a type other than `integer`.
///
/// ## Why is this bad?
/// The `__index__` method should return an `integer`. Returning a different
/// type may cause unexpected behavior.
///
/// ## Example
/// ```python
/// class Foo:
///     def __index__(self):
///         return "2"
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __index__(self):
///         return 2
/// ```
///
/// Note: Strictly speaking `bool` is a subclass of `int`, thus returning `True`/`False` is valid.
/// However, a DeprecationWarning (`DeprecationWarning: __index__ returned non-int (type bool)`)
/// for such cases was already introduced, thus this is a conscious difference between the original
/// pylint rule and the current ruff implementation.
/// ## References
/// - [Python documentation: The `__index__` method](https://docs.python.org/3/reference/datamodel.html#object.__index__)
#[violation]
pub struct InvalidIndexReturnType;

impl Violation for InvalidIndexReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__index__` does not return `integer`")
    }
}

/// E0305
pub(crate) fn invalid_index_return(checker: &mut Checker, name: &str, body: &[Stmt]) {
    if name != "__index__" {
        return;
    }

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }

    if body.len() == 1
        && (matches!(&body[0], Stmt::Expr(expr) if expr.value.is_ellipsis_literal_expr())
            || body[0].is_pass_stmt()
            || body[0].is_raise_stmt())
    {
        return;
    }

    let body_without_comments = body
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Expr(expr) if expr.value.is_string_literal_expr()))
        .collect::<Vec<_>>();
    if body_without_comments.is_empty() {
        return;
    }
    if body_without_comments.len() == 1 && body_without_comments[0].is_raise_stmt() {
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    if returns.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            InvalidIndexReturnType,
            body.last().unwrap().range(),
        ));
    }

    for stmt in returns {
        if let Some(value) = stmt.value.as_deref() {
            if !matches!(
                ResolvedPythonType::from(value),
                ResolvedPythonType::Unknown
                    | ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
            ) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidIndexReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidIndexReturnType, stmt.range()));
        }
    }
}
