use rustpython_ast::{Expr, ExprKind, Keyword, Stmt};

use super::super::fixes;
use crate::ast::types::{Binding, BindingKind, Range, Scope};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn rule(name: &str, bases: &[Expr], scope: &Scope, bindings: &[Binding]) -> Option<Diagnostic> {
    for expr in bases {
        let ExprKind::Name { id, .. } = &expr.node else {
            continue;
        };
        if id != "object" {
            continue;
        }
        if !matches!(
            scope
                .values
                .get(&id.as_str())
                .map(|index| &bindings[*index]),
            None | Some(Binding {
                kind: BindingKind::Builtin,
                ..
            })
        ) {
            continue;
        }
        return Some(Diagnostic::new(
            violations::UselessObjectInheritance(name.to_string()),
            Range::from_located(expr),
        ));
    }

    None
}

/// UP004
pub fn useless_object_inheritance(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
) {
    let Some(mut diagnostic) = rule(name, bases, checker.current_scope(), &checker.bindings) else {
        return;
    };
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(fix) = fixes::remove_class_def_base(
            checker.locator,
            stmt.location,
            diagnostic.location,
            bases,
            keywords,
        ) {
            diagnostic.amend(fix);
        }
    }
    checker.diagnostics.push(diagnostic);
}
