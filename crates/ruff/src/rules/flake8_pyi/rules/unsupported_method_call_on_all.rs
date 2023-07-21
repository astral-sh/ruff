use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{self, Expr, ExprAttribute, Ranged};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that `append`, `extend` and `remove` methods are not
/// called on `__all__`.
///
/// ## Why is this bad?
/// Different type checkers have varying levels of support for
/// calling these methods on `__all__`.
///
/// ## Example
/// ```python
/// __all__ = ["A"]
/// __all__.append("B")
/// ```
///
/// Use instead:
/// ```python
/// __all__ = ["A"]
/// __all__ += "B"
/// ```
#[violation]
pub struct UnsupportedMethodCallOnAll {
    name: String,
}

impl Violation for UnsupportedMethodCallOnAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsupportedMethodCallOnAll { name } = self;
        format!("Calling \".{name}()\" on \"__all__\" may not be supported by all type checkers (use += instead)")
    }
}

fn is_unsupported_method(name: &str) -> bool {
    matches!(name, "append" | "extend" | "remove")
}

pub(crate) fn unsupported_method_call_on_all(checker: &mut Checker, func: &Expr) {
    let Expr::Attribute(attribute) = func else {
        return;
    };

    let ExprAttribute { value, attr, .. } = attribute;

    if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
        if id.as_str() != "__all__" {
            return;
        }
    }
    if !is_unsupported_method(attr.as_str()) {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        UnsupportedMethodCallOnAll {
            name: attr.to_string(),
        },
        func.range(),
    ));
}
