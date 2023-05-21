use rustpython_parser::ast::{self, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::traits::AstRule;
use crate::checkers::ast::RuleContext;
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

impl AstRule<ast::ExprCall> for TypeOfPrimitive {
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &ast::ExprCall) {
        type_of_primitive(diagnostics, checker, node)
    }
}

/// UP003
pub(crate) fn type_of_primitive(
    diagnostics: &mut Vec<Diagnostic>,
    checker: &RuleContext,
    ast::ExprCall {
        func, args, range, ..
    }: &ast::ExprCall,
) {
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
    let Expr::Constant(ast::ExprConstant { value, .. } )= &args[0] else {
        return;
    };
    let Some(primitive) = Primitive::from_constant(value) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(TypeOfPrimitive { primitive }, *range);
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            primitive.builtin(),
            *range,
        )));
    }
    diagnostics.push(diagnostic);
}
