use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

use super::super::types::Primitive;

#[violation]
pub struct TypeOfPrimitive {
    pub primitive: Primitive,
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
pub fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if args.len() != 1 {
        return;
    }
    if !checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "type"])
    {
        return;
    }
    let ExprKind::Constant { value, .. } = &args[0].node else {
        return;
    };
    let Some(primitive) = Primitive::from_constant(value) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(TypeOfPrimitive { primitive }, Range::from(expr));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            primitive.builtin(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
