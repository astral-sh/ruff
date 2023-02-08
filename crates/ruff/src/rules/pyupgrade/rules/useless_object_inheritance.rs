use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Keyword, Stmt};

use super::super::fixes;
use crate::ast::types::{Binding, BindingKind, Range, Scope};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UselessObjectInheritance {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UselessObjectInheritance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessObjectInheritance { name } = self;
        format!("Class `{name}` inherits from `object`")
    }

    fn autofix_title(&self) -> String {
        "Remove `object` inheritance".to_string()
    }
}

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
                .bindings
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
            UselessObjectInheritance {
                name: name.to_string(),
            },
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
            diagnostic.end_location,
            bases,
            keywords,
        ) {
            diagnostic.amend(fix);
        }
    }
    checker.diagnostics.push(diagnostic);
}
