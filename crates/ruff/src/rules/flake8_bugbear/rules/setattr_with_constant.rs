use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_stmt;
use ruff_python_ast::source_code::Stylist;
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SetAttrWithConstant;

impl AlwaysAutofixableViolation for SetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `setattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn autofix_title(&self) -> String {
        "Replace `setattr` with assignment".to_string()
    }
}

fn assignment(obj: &Expr, name: &str, value: &Expr, stylist: &Stylist) -> String {
    let stmt = Stmt::new(
        TextRange::default(),
        StmtKind::Assign(ast::StmtAssign {
            targets: vec![Expr::new(
                TextRange::default(),
                ExprKind::Attribute(ast::ExprAttribute {
                    value: Box::new(obj.clone()),
                    attr: name.into(),
                    ctx: ExprContext::Store,
                }),
            )],
            value: Box::new(value.clone()),
            type_comment: None,
        }),
    );
    unparse_stmt(&stmt, stylist)
}

/// B010
pub(crate) fn setattr_with_constant(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else {
        return;
    };
    if id != "setattr" {
        return;
    }
    let [obj, name, value] = args else {
        return;
    };
    let ExprKind::Constant(ast::ExprConstant {
        value: Constant::Str(name),
        ..
    } )= &name.node else {
        return;
    };
    if !is_identifier(name) {
        return;
    }
    if is_mangled_private(name.as_str()) {
        return;
    }
    // We can only replace a `setattr` call (which is an `Expr`) with an assignment
    // (which is a `Stmt`) if the `Expr` is already being used as a `Stmt`
    // (i.e., it's directly within an `StmtKind::Expr`).
    if let StmtKind::Expr(ast::StmtExpr { value: child }) = &checker.ctx.stmt().node {
        if expr == child.as_ref() {
            let mut diagnostic = Diagnostic::new(SetAttrWithConstant, expr.range());

            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    assignment(obj, name, value, checker.stylist),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
