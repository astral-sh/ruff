use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;
use crate::registry::Diagnostic;
use crate::source_code_generator::SourceCodeGenerator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::violations;

fn assignment(obj: &Expr, name: &str, value: &Expr, stylist: &SourceCodeStyleDetector) -> String {
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
    let mut generator: SourceCodeGenerator = stylist.into();
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
            let mut check =
                Diagnostic::new(violations::SetAttrWithConstant, Range::from_located(expr));
            if checker.patch(check.kind.code()) {
                check.amend(Fix::replacement(
                    assignment(obj, name, value, checker.style),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.checks.push(check);
        }
    }
}
