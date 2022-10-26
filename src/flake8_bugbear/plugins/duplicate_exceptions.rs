use std::collections::BTreeSet;

use itertools::Itertools;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind, Stmt};

use crate::ast::helpers;
use crate::ast::types::{CheckLocator, Range};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::code_gen::SourceGenerator;

fn type_pattern(elts: Vec<&Expr>) -> Expr {
    Expr::new(
        Default::default(),
        Default::default(),
        ExprKind::Tuple {
            elts: elts.into_iter().cloned().collect(),
            ctx: ExprContext::Load,
        },
    )
}

pub fn duplicate_handler_exceptions(
    checker: &mut Checker,
    expr: &Expr,
    elts: &[Expr],
) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = Default::default();
    let mut duplicates: BTreeSet<String> = Default::default();
    let mut unique_elts: Vec<&Expr> = Default::default();
    for type_ in elts {
        if let Some(name) = helpers::compose_call_path(type_) {
            if seen.contains(&name) {
                duplicates.insert(name);
            } else {
                seen.insert(name);
                unique_elts.push(type_);
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::B014) {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut check = Check::new(
                CheckKind::DuplicateHandlerException(
                    duplicates.into_iter().sorted().collect::<Vec<String>>(),
                ),
                checker.locate_check(Range::from_located(expr)),
            );
            if checker.patch() {
                // TODO(charlie): If we have a single element, remove the tuple.
                let mut generator = SourceGenerator::new();
                if let Ok(()) = generator.unparse_expr(&type_pattern(unique_elts), 0) {
                    if let Ok(content) = generator.generate() {
                        check.amend(Fix::replacement(
                            content,
                            expr.location,
                            expr.end_location.unwrap(),
                        ))
                    }
                }
            }
            checker.add_check(check);
        }
    }

    seen
}

pub fn duplicate_exceptions(checker: &mut Checker, stmt: &Stmt, handlers: &[Excepthandler]) {
    let mut seen: BTreeSet<String> = Default::default();
    let mut duplicates: BTreeSet<String> = Default::default();
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    match &type_.node {
                        ExprKind::Attribute { .. } | ExprKind::Name { .. } => {
                            if let Some(name) = helpers::compose_call_path(type_) {
                                if seen.contains(&name) {
                                    duplicates.insert(name);
                                } else {
                                    seen.insert(name);
                                }
                            }
                        }
                        ExprKind::Tuple { elts, .. } => {
                            for name in duplicate_handler_exceptions(checker, type_, elts) {
                                if seen.contains(&name) {
                                    duplicates.insert(name);
                                } else {
                                    seen.insert(name);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::B025) {
        for duplicate in duplicates.into_iter().sorted() {
            checker.add_check(Check::new(
                CheckKind::DuplicateTryBlockException(duplicate),
                checker.locate_check(Range::from_located(stmt)),
            ));
        }
    }
}
