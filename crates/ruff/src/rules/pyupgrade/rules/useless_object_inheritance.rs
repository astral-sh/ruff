use rustpython_parser::ast::{Expr, ExprKind, Keyword, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::binding::{Binding, BindingKind, Bindings};
use ruff_python_semantic::scope::Scope;

use crate::autofix::actions::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UselessObjectInheritance {
    pub name: String,
}

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

fn rule(name: &str, bases: &[Expr], scope: &Scope, bindings: &Bindings) -> Option<Diagnostic> {
    for expr in bases {
        let ExprKind::Name { id, .. } = &expr.node else {
            continue;
        };
        if id != "object" {
            continue;
        }
        if !matches!(
            scope.get(id.as_str()).map(|index| &bindings[*index]),
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
            Range::from(expr),
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
    if let Some(mut diagnostic) = rule(name, bases, checker.ctx.scope(), &checker.ctx.bindings) {
        if checker.patch(diagnostic.kind.rule()) {
            let location = diagnostic.location;
            let end_location = diagnostic.end_location;
            diagnostic.try_set_fix(|| {
                remove_argument(
                    checker.locator,
                    stmt.location,
                    location,
                    end_location,
                    bases,
                    keywords,
                    true,
                )
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
