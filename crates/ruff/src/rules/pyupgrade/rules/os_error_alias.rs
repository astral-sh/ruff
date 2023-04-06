use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::types::Range;
use ruff_python_semantic::context::Context;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct OSErrorAlias {
    pub name: Option<String>,
}

impl AlwaysAutofixableViolation for OSErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `OSError`")
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias { name } = self;
        match name {
            None => "Replace with builtin `OSError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `OSError`"),
        }
    }
}

const ALIASES: &[(&str, &str)] = &[
    ("", "EnvironmentError"),
    ("", "IOError"),
    ("", "WindowsError"),
    ("mmap", "error"),
    ("select", "error"),
    ("socket", "error"),
];

/// Return `true` if an [`Expr`] is an alias of `OSError`.
fn is_alias(context: &Context, expr: &Expr) -> bool {
    context.resolve_call_path(expr).map_or(false, |call_path| {
        ALIASES
            .iter()
            .any(|(module, member)| call_path.as_slice() == [*module, *member])
    })
}

/// Return `true` if an [`Expr`] is `OSError`.
fn is_os_error(context: &Context, expr: &Expr) -> bool {
    context
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["", "OSError"])
}

/// Create a [`Diagnostic`] for a single target, like an [`ExprKind::Name`].
fn atom_diagnostic(checker: &mut Checker, target: &Expr) {
    let mut diagnostic = Diagnostic::new(
        OSErrorAlias {
            name: compose_call_path(target),
        },
        Range::from(target),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            "OSError".to_string(),
            target.location,
            target.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &mut Checker, target: &Expr, aliases: &[&Expr]) {
    let mut diagnostic = Diagnostic::new(OSErrorAlias { name: None }, Range::from(target));
    if checker.patch(diagnostic.kind.rule()) {
        let ExprKind::Tuple { elts, ..} = &target.node else {
            panic!("Expected ExprKind::Tuple");
        };

        // Filter out any `OSErrors` aliases.
        let mut remaining: Vec<Expr> = elts
            .iter()
            .filter_map(|elt| {
                if aliases.contains(&elt) {
                    None
                } else {
                    Some(elt.clone())
                }
            })
            .collect();

        // If `OSError` itself isn't already in the tuple, add it.
        if elts.iter().all(|elt| !is_os_error(&checker.ctx, elt)) {
            remaining.insert(
                0,
                create_expr(ExprKind::Name {
                    id: "OSError".to_string(),
                    ctx: ExprContext::Load,
                }),
            );
        }

        if remaining.len() == 1 {
            diagnostic.set_fix(Edit::replacement(
                "OSError".to_string(),
                target.location,
                target.end_location.unwrap(),
            ));
        } else {
            diagnostic.set_fix(Edit::replacement(
                format!(
                    "({})",
                    unparse_expr(
                        &create_expr(ExprKind::Tuple {
                            elts: remaining,
                            ctx: ExprContext::Load,
                        }),
                        checker.stylist,
                    )
                ),
                target.location,
                target.end_location.unwrap(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// UP024
pub fn os_error_alias_handlers(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match &expr.node {
            ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
                if is_alias(&checker.ctx, expr) {
                    atom_diagnostic(checker, expr);
                }
            }
            ExprKind::Tuple { elts, .. } => {
                // List of aliases to replace with `OSError`.
                let mut aliases: Vec<&Expr> = vec![];
                for elt in elts {
                    if is_alias(&checker.ctx, elt) {
                        aliases.push(elt);
                    }
                }
                if !aliases.is_empty() {
                    tuple_diagnostic(checker, expr, &aliases);
                }
            }
            _ => {}
        }
    }
}

/// UP024
pub fn os_error_alias_call(checker: &mut Checker, func: &Expr) {
    if is_alias(&checker.ctx, func) {
        atom_diagnostic(checker, func);
    }
}

/// UP024
pub fn os_error_alias_raise(checker: &mut Checker, expr: &Expr) {
    if matches!(
        expr.node,
        ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
        if is_alias(&checker.ctx, expr) {
            atom_diagnostic(checker, expr);
        }
    }
}
