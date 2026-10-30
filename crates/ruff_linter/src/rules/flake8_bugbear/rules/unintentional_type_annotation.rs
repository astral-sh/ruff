use ruff_python_ast::{Expr, Stmt, flat};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.250", category = Category::Suspicious)]
pub(crate) struct UnintentionalTypeAnnotation;

impl Violation for UnintentionalTypeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Possible unintentional type annotation (using `:`). Did you mean to assign (using `=`)?"
            .to_string()
    }
}

/// B032
pub(crate) fn unintentional_type_annotation(
    checker: &Checker,
    target: flat::View<'_, Expr>,
    value: Option<flat::View<'_, Expr>>,
    stmt: flat::View<'_, Stmt>,
) {
    if value.is_some() {
        return;
    }
    match target.classify() {
        flat::ExprKind::Subscript(expr_subscript) if expr_subscript.value().is_name_expr() => {
            checker.report_diagnostic(UnintentionalTypeAnnotation, stmt.range());
        }
        flat::ExprKind::Attribute(expr_attribute) => {
            let value = expr_attribute.value();

            if let flat::ExprKind::Name(name) = value.classify() {
                let id = name.id();
                if id != "self" {
                    checker.report_diagnostic(UnintentionalTypeAnnotation, stmt.range());
                }
            }
        }
        _ => {}
    }
}
