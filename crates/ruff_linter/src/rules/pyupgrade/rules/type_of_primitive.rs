use ruff_python_ast::Expr;

use crate::fix::edits::pad;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::types::Primitive;

/// ## What it does
/// Checks for uses of `type` that take a primitive as an argument.
///
/// ## Why is this bad?
/// `type()` returns the type of a given object. A type of a primitive can
/// always be known in advance and accessed directly, which is more concise
/// and explicit than using `type()`.
///
/// ## Example
/// ```python
/// type(1)
/// ```
///
/// Use instead:
/// ```python
/// int
/// ```
///
/// ## References
/// - [Python documentation: `type()`](https://docs.python.org/3/library/functions.html#type)
/// - [Python documentation: Built-in types](https://docs.python.org/3/library/stdtypes.html)
#[violation]
pub struct TypeOfPrimitive {
    primitive: Primitive,
}

impl Violation for TypeOfPrimitive {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeOfPrimitive { primitive } = self;
        format!("Use `{}` instead of `type(...)`", primitive.builtin())
    }

    fn fix_title(&self) -> Option<String> {
        let TypeOfPrimitive { primitive } = self;
        Some(format!(
            "Replace `type(...)` with `{}`",
            primitive.builtin()
        ))
    }
}

/// UP003
pub(crate) fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let [arg] = args else {
        return;
    };
    let Some(primitive) = Primitive::from_expr(arg) else {
        return;
    };
    let semantic = checker.semantic();
    if !semantic.match_builtin_expr(func, "type") {
        return;
    }
    let mut diagnostic = Diagnostic::new(TypeOfPrimitive { primitive }, expr.range());
    let builtin = primitive.builtin();
    if semantic.has_builtin_binding(&builtin) {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(primitive.builtin(), expr.range(), checker.locator()),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
