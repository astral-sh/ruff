use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Generator;
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

fn assignment(obj: &Expr, name: &str, value: &Expr, generator: Generator) -> String {
    let stmt = Stmt::Assign(ast::StmtAssign {
        targets: vec![Expr::Attribute(ast::ExprAttribute {
            value: Box::new(obj.clone()),
            attr: name.into(),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })],
        value: Box::new(value.clone()),
        type_comment: None,
        range: TextRange::default(),
    });
    generator.stmt(&stmt)
}

/// B010
pub(crate) fn setattr_with_constant(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };
    if id != "setattr" {
        return;
    }
    let [obj, name, value] = args else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(name),
        ..
    } )= name else {
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
    // (i.e., it's directly within an `Stmt::Expr`).
    if let Stmt::Expr(ast::StmtExpr {
        value: child,
        range: _,
    }) = checker.semantic_model().stmt()
    {
        if expr == child.as_ref() {
            let mut diagnostic = Diagnostic::new(SetAttrWithConstant, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    assignment(obj, name, value, checker.generator()),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
