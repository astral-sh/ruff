use rustpython_parser::ast::{Expr, ExprKind, Stmt};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for the unintentional use of type annotations.
    ///
    /// ## Why is this bad?
    /// The use of a colon (`:`) in lieu of an assignment (`=`) can be syntactically valid, but
    /// is almost certainly a mistake when used in a subscript or attribute assignment.
    ///
    /// ## Example
    /// ```python
    /// a["b"]: 1
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// a["b"] = 1
    /// ```
    pub struct UnintentionalTypeAnnotation;
);
impl Violation for UnintentionalTypeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Possible unintentional type annotation (using `:`). Did you mean to assign (using `=`)?"
        )
    }
}

/// B032
pub fn unintentional_type_annotation(
    checker: &mut Checker,
    target: &Expr,
    value: Option<&Expr>,
    stmt: &Stmt,
) {
    if value.is_some() {
        return;
    }
    match &target.node {
        ExprKind::Subscript { value, .. } => {
            if matches!(&value.node, ExprKind::Name { .. }) {
                checker.diagnostics.push(Diagnostic::new(
                    UnintentionalTypeAnnotation,
                    Range::from_located(stmt),
                ));
            }
        }
        ExprKind::Attribute { value, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                if id != "self" {
                    checker.diagnostics.push(Diagnostic::new(
                        UnintentionalTypeAnnotation,
                        Range::from_located(stmt),
                    ));
                }
            }
        }
        _ => {}
    };
}
