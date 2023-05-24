use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

use super::super::types::Primitive;

#[violation]
pub struct TypeOfPrimitive {
    primitive: Primitive,
}

impl AlwaysAutofixableViolation for TypeOfPrimitive {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeOfPrimitive { primitive } = self;
        format!("Use `{}` instead of `type(...)`", primitive.builtin())
    }

    fn autofix_title(&self) -> String {
        let TypeOfPrimitive { primitive } = self;
        format!("Replace `type(...)` with `{}`", primitive.builtin())
    }
}

/// UP003
pub(crate) fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if args.len() != 1 {
        return;
    }
    if !checker
        .semantic_model()
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "type"])
    {
        return;
    }
    let Expr::Constant(ast::ExprConstant { value, .. } )= &args[0] else {
        return;
    };
    let Some(primitive) = Primitive::from_constant(value) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(TypeOfPrimitive { primitive }, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            primitive.builtin(),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
