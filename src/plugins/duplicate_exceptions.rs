use itertools::Itertools;
use std::collections::BTreeSet;

use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt};

use crate::ast::helpers;
use crate::ast::types::{CheckLocator, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

pub fn duplicate_handler_exceptions(
    checker: &mut Checker,
    stmt: &Stmt,
    elts: &Vec<Expr>,
) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = Default::default();
    let mut duplicates: BTreeSet<String> = Default::default();
    for type_ in elts {
        if let Some(name) = helpers::compose_call_path(type_) {
            if seen.contains(&name) {
                duplicates.insert(name);
            } else {
                seen.insert(name);
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::B014) {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        for duplicate in duplicates.into_iter().sorted() {
            checker.add_check(Check::new(
                CheckKind::DuplicateHandlerException(duplicate),
                checker.locate_check(Range::from_located(stmt)),
            ));
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
                            for name in duplicate_handler_exceptions(checker, stmt, elts) {
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
