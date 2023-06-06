use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct GetAttrWithConstant;

impl AlwaysAutofixableViolation for GetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `getattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn autofix_title(&self) -> String {
        "Replace `getattr` with attribute access".to_string()
    }
}
fn attribute(value: &Expr, attr: &str) -> Expr {
    ast::ExprAttribute {
        value: Box::new(value.clone()),
        attr: attr.into(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    }
    .into()
}

/// B009
pub(crate) fn getattr_with_constant(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Expr::Name(ast::ExprName { id, .. } )= func else {
        return;
    };
    if id != "getattr" {
        return;
    }
    let [obj, arg] = args else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    } )= arg else {
        return;
    };
    if !is_identifier(value) {
        return;
    }
    if is_mangled_private(value.as_str()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(GetAttrWithConstant, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            checker.generator().expr(&attribute(obj, value)),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
