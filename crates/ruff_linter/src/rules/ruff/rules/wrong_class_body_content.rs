use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{helpers::is_docstring_stmt, Stmt, StmtClassDef, StmtExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for disallowed statements in the body of a class.
///
/// ## Why is this bad?
/// Python allows us to have conditions, context managers,
/// and even infinite loops inside class definitions.
/// On the other hand, only methods, attributes, and docstrings make sense.
/// So, we discourage using anything except these nodes in class bodies.
///
/// ## Example
/// ```python
/// class Test:
///     for _ in range(10):
///         print("What?!")
/// ```
///
/// ## References
/// - [WPS: wrong class body content](https://wemake-python-styleguide.readthedocs.io/en/0.19.2/pages/usage/violations/oop.html#wemake_python_styleguide.violations.oop.WrongClassBodyContentViolation)
#[violation]
pub struct WrongClassBodyContent;

impl Violation for WrongClassBodyContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Wrong statement inside class definition".to_string()
    }
}

/// RUF050
pub(crate) fn wrong_class_body_content(checker: &mut Checker, class: &StmtClassDef) {
    let StmtClassDef { body, .. } = class;
    let is_stub = checker.source_type.is_stub();
    for stmt in body {
        if !is_docstring_stmt(stmt) && !is_allowed_statement(stmt, is_stub) {
            checker
                .diagnostics
                .push(Diagnostic::new(WrongClassBodyContent, stmt.range()));
        }
    }
}

fn is_allowed_statement(stmt: &Stmt, is_stub: bool) -> bool {
    match stmt {
        Stmt::FunctionDef(_)
        | Stmt::ClassDef(_)
        | Stmt::Assign(_)
        | Stmt::AnnAssign(_)
        | Stmt::Pass(_) => true,
        Stmt::Expr(StmtExpr { value, .. }) => value.is_ellipsis_literal_expr(),
        Stmt::If(_) => {
            // If statement are allowed in stubs
            is_stub
            // TODO: allow also use of if for type checking
        }
        _ => false,
    }
}
