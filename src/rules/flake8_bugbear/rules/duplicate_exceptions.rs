use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind, Location};

use crate::ast::helpers;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::{CallPath, Range};
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

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
        let call_path = helpers::collect_call_path(type_);
        if !call_path.is_empty() {
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
        .enabled(&Rule::DuplicateHandlerException)
    {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut diagnostic = Diagnostic::new(
                violations::DuplicateHandlerException(
                    duplicates
                        .into_iter()
                        .map(|call_path| call_path.join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                ),
                Range::from_located(expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::replacement(
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
                let call_path = helpers::collect_call_path(type_);
                if !call_path.is_empty() {
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
        .enabled(&Rule::DuplicateTryBlockException)
    {
        for (name, exprs) in duplicates {
            for expr in exprs {
                checker.diagnostics.push(Diagnostic::new(
                    violations::DuplicateTryBlockException(name.join(".")),
                    Range::from_located(expr),
                ));
            }
        }
    }
}
