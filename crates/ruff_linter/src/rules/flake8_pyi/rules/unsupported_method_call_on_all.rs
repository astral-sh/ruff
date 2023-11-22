use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that `append`, `extend` and `remove` methods are not called on
/// `__all__`.
///
/// ## Why is this bad?
/// Different type checkers have varying levels of support for calling these
/// methods on `__all__`. Instead, use the `+=` operator to add items to
/// `__all__`, which is known to be supported by all major type checkers.
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
/// __all__ += ["B"]
/// ```
#[violation]
pub struct UnsupportedMethodCallOnAll {
    name: String,
}

impl Violation for UnsupportedMethodCallOnAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsupportedMethodCallOnAll { name } = self;
        format!("Calling `.{name}()` on `__all__` may not be supported by all type checkers (use `+=` instead)")
    }
}

/// PYI056
pub(crate) fn unsupported_method_call_on_all(checker: &mut Checker, func: &Expr) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    if id.as_str() != "__all__" {
        return;
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

fn is_unsupported_method(name: &str) -> bool {
    matches!(name, "append" | "extend" | "remove")
}
