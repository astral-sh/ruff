use rustpython_parser::ast::{self, Expr, Keyword, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::{Binding, BindingKind, Bindings};
use ruff_python_semantic::scope::Scope;

use crate::autofix::actions::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UselessObjectInheritance {
    name: String,
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
        let Expr::Name(ast::ExprName { id, .. }) = expr else {
            continue;
        };
        if id != "object" {
            continue;
        }
        if !matches!(
            scope
                .get(id.as_str())
                .map(|binding_id| &bindings[binding_id]),
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
            expr.range(),
        ));
    }

    None
}

/// UP004
pub(crate) fn useless_object_inheritance(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(mut diagnostic) = rule(
        name,
        bases,
        checker.semantic_model().scope(),
        &checker.semantic_model().bindings,
    ) {
        if checker.patch(diagnostic.kind.rule()) {
            let expr_range = diagnostic.range();
            #[allow(deprecated)]
            diagnostic.try_set_fix_from_edit(|| {
                remove_argument(
                    checker.locator,
                    stmt.start(),
                    expr_range,
                    bases,
                    keywords,
                    true,
                )
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
