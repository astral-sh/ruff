use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;
use crate::registry::Diagnostic;
use crate::source_code::{Generator, Stylist};
use crate::violations;

fn assignment(obj: &Expr, name: &str, value: &Expr, stylist: &Stylist) -> String {
    let stmt = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Assign {
            targets: vec![Expr::new(
                Location::default(),
                Location::default(),
                ExprKind::Attribute {
                    value: Box::new(obj.clone()),
                    attr: name.to_string(),
                    ctx: ExprContext::Store,
                },
            )],
            value: Box::new(value.clone()),
            type_comment: None,
        },
    );
    let mut generator: Generator = stylist.into();
    generator.unparse_stmt(&stmt);
    generator.generate()
}

/// B010
pub fn setattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    if id != "setattr" {
        return;
    }
    let [obj, name, value] = args else {
        return;
    };
    let ExprKind::Constant {
        value: Constant::Str(name),
        ..
    } = &name.node else {
        return;
    };
    if !IDENTIFIER_REGEX.is_match(name) {
        return;
    }
    if KWLIST.contains(&name.as_str()) {
        return;
    }
    // We can only replace a `setattr` call (which is an `Expr`) with an assignment
    // (which is a `Stmt`) if the `Expr` is already being used as a `Stmt`
    // (i.e., it's directly within an `StmtKind::Expr`).
    if let StmtKind::Expr { value: child } = &checker.current_stmt().node {
        if expr == child.as_ref() {
            let mut diagnostic =
                Diagnostic::new(violations::SetAttrWithConstant, Range::from_located(expr));
            if checker.patch(diagnostic.kind.code()) {
                diagnostic.amend(Fix::replacement(
                    assignment(obj, name, value, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
