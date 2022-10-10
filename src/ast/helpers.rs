use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, StmtKind};

use crate::python::typing;

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript(expr: &Expr) -> Option<SubscriptKind> {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => {
            if typing::is_annotated_subscript(attr) {
                Some(SubscriptKind::AnnotatedSubscript)
            } else if typing::is_pep593_annotated_subscript(attr) {
                Some(SubscriptKind::PEP593AnnotatedSubscript)
            } else {
                None
            }
        }
        ExprKind::Name { id, .. } => {
            if typing::is_annotated_subscript(id) {
                Some(SubscriptKind::AnnotatedSubscript)
            } else if typing::is_pep593_annotated_subscript(id) {
                Some(SubscriptKind::PEP593AnnotatedSubscript)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn node_name(expr: &Expr) -> Option<&str> {
    if let ExprKind::Name { id, .. } = &expr.node {
        Some(id)
    } else {
        None
    }
}

pub fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
    }
}

pub fn is_assignment_to_a_dunder(node: &StmtKind) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type annotation.
    // This is what pycodestyle (as of 2.9.1) does.
    match node {
        StmtKind::Assign {
            targets,
            value: _,
            type_comment: _,
        } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign {
            target,
            annotation: _,
            value: _,
            simple: _,
        } => match &target.node {
            ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

/// Extract the names of all handled exceptions.
/// Note that, for now, this only matches on ExprKind::Name, and so won't catch exceptions like
/// `module.CustomException`. (But will catch all builtin exceptions.)
pub fn extract_handler_names(handlers: &[Excepthandler]) -> Vec<&str> {
    let mut handler_names = vec![];
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    if let ExprKind::Tuple { elts, .. } = &type_.node {
                        for type_ in elts {
                            if let Some(name) = node_name(type_) {
                                handler_names.push(name);
                            }
                        }
                    } else if let Some(name) = node_name(type_) {
                        handler_names.push(name);
                    }
                }
            }
        }
    }
    handler_names
}
