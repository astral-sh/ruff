use itertools::Itertools;
use rustc_hash::FxHashSet;
use rustpython_ast::{
    Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind, Location, Stmt,
};

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::code_gen::SourceGenerator;

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
) -> FxHashSet<Vec<&'a str>> {
    let mut seen: FxHashSet<Vec<&str>> = FxHashSet::default();
    let mut duplicates: FxHashSet<Vec<&str>> = FxHashSet::default();
    let mut unique_elts: Vec<&Expr> = Vec::default();
    for type_ in elts {
        let call_path = helpers::collect_call_paths(type_);
        if !call_path.is_empty() {
            if seen.contains(&call_path) {
                duplicates.insert(call_path);
            } else {
                seen.insert(call_path);
                unique_elts.push(type_);
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::B014) {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut check = Check::new(
                CheckKind::DuplicateHandlerException(
                    duplicates
                        .into_iter()
                        .map(|call_path| call_path.join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                ),
                Range::from_located(expr),
            );
            if checker.patch(check.kind.code()) {
                let mut generator = SourceGenerator::new();
                if unique_elts.len() == 1 {
                    generator.unparse_expr(unique_elts[0], 0);
                } else {
                    generator.unparse_expr(&type_pattern(unique_elts), 0);
                }
                if let Ok(content) = generator.generate() {
                    check.amend(Fix::replacement(
                        content,
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
            }
            checker.add_check(check);
        }
    }

    seen
}

pub fn duplicate_exceptions(checker: &mut Checker, stmt: &Stmt, handlers: &[Excepthandler]) {
    let mut seen: FxHashSet<Vec<&str>> = FxHashSet::default();
    let mut duplicates: FxHashSet<Vec<&str>> = FxHashSet::default();
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_: Some(type_), .. } = &handler.node else {
            continue;
        };
        match &type_.node {
            ExprKind::Attribute { .. } | ExprKind::Name { .. } => {
                let call_path = helpers::collect_call_paths(type_);
                if !call_path.is_empty() {
                    if seen.contains(&call_path) {
                        duplicates.insert(call_path);
                    } else {
                        seen.insert(call_path);
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

    if checker.settings.enabled.contains(&CheckCode::B025) {
        for duplicate in duplicates.into_iter().sorted() {
            checker.add_check(Check::new(
                CheckKind::DuplicateTryBlockException(duplicate.join(".")),
                Range::from_located(stmt),
            ));
        }
    }
}
