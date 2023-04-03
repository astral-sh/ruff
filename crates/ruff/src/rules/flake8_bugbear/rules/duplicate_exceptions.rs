use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{
    Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind, Location,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path;
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct DuplicateTryBlockException {
    pub name: String,
}

impl Violation for DuplicateTryBlockException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateTryBlockException { name } = self;
        format!("try-except block with duplicate exception `{name}`")
    }
}
#[violation]
pub struct DuplicateHandlerException {
    pub names: Vec<String>,
}

impl AlwaysAutofixableViolation for DuplicateHandlerException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateHandlerException { names } = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Exception handler with duplicate exception: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Exception handler with duplicate exceptions: {names}")
        }
    }

    fn autofix_title(&self) -> String {
        "De-duplicate exceptions".to_string()
    }
}

fn type_pattern(elts: Vec<&Expr>) -> Expr {
    Expr::new(
        Location::default(),
        Location::default(),
        ExprKind::Tuple {
            elts: elts.into_iter().cloned().collect(),
            ctx: ExprContext::Load,
        },
    )
}

fn duplicate_handler_exceptions<'a>(
    checker: &mut Checker,
    expr: &'a Expr,
    elts: &'a [Expr],
) -> FxHashMap<CallPath<'a>, &'a Expr> {
    let mut seen: FxHashMap<CallPath, &Expr> = FxHashMap::default();
    let mut duplicates: FxHashSet<CallPath> = FxHashSet::default();
    let mut unique_elts: Vec<&Expr> = Vec::default();
    for type_ in elts {
        if let Some(call_path) = call_path::collect_call_path(type_) {
            if seen.contains_key(&call_path) {
                duplicates.insert(call_path);
            } else {
                seen.entry(call_path).or_insert(type_);
                unique_elts.push(type_);
            }
        }
    }

    if checker
        .settings
        .rules
        .enabled(Rule::DuplicateHandlerException)
    {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut diagnostic = Diagnostic::new(
                DuplicateHandlerException {
                    names: duplicates
                        .into_iter()
                        .map(|call_path| call_path.join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                },
                Range::from(expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Edit::replacement(
                    if unique_elts.len() == 1 {
                        unparse_expr(unique_elts[0], checker.stylist)
                    } else {
                        unparse_expr(&type_pattern(unique_elts), checker.stylist)
                    },
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    seen
}

pub fn duplicate_exceptions(checker: &mut Checker, handlers: &[Excepthandler]) {
    let mut seen: FxHashSet<CallPath> = FxHashSet::default();
    let mut duplicates: FxHashMap<CallPath, Vec<&Expr>> = FxHashMap::default();
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_: Some(type_), .. } = &handler.node else {
            continue;
        };
        match &type_.node {
            ExprKind::Attribute { .. } | ExprKind::Name { .. } => {
                if let Some(call_path) = call_path::collect_call_path(type_) {
                    if seen.contains(&call_path) {
                        duplicates.entry(call_path).or_default().push(type_);
                    } else {
                        seen.insert(call_path);
                    }
                }
            }
            ExprKind::Tuple { elts, .. } => {
                for (name, expr) in duplicate_handler_exceptions(checker, type_, elts) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(expr);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            _ => {}
        }
    }

    if checker
        .settings
        .rules
        .enabled(Rule::DuplicateTryBlockException)
    {
        for (name, exprs) in duplicates {
            for expr in exprs {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateTryBlockException {
                        name: name.join("."),
                    },
                    Range::from(expr),
                ));
            }
        }
    }
}
