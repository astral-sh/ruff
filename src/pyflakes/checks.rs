use std::string::ToString;

use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::except_range;
use crate::ast::types::{Binding, Range, Scope, ScopeKind};
use crate::registry::Diagnostic;
use crate::source_code_locator::SourceCodeLocator;
use crate::violations;

/// F631
pub fn assert_tuple(test: &Expr, location: Range) -> Option<Diagnostic> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Diagnostic::new(violations::AssertTuple, location));
        }
    }
    None
}

/// F634
pub fn if_tuple(test: &Expr, location: Range) -> Option<Diagnostic> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Diagnostic::new(violations::IfTuple, location));
        }
    }
    None
}

/// F821
pub fn undefined_local(name: &str, scopes: &[&Scope], bindings: &[Binding]) -> Option<Diagnostic> {
    let current = &scopes.last().expect("No current scope found");
    if matches!(current.kind, ScopeKind::Function(_)) && !current.values.contains_key(name) {
        for scope in scopes.iter().rev().skip(1) {
            if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
                if let Some(binding) = scope.values.get(name).map(|index| &bindings[*index]) {
                    if let Some((scope_id, location)) = binding.used {
                        if scope_id == current.id {
                            return Some(Diagnostic::new(
                                violations::UndefinedLocal(name.to_string()),
                                location,
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

/// F707
pub fn default_except_not_last(
    handlers: &[Excepthandler],
    locator: &SourceCodeLocator,
) -> Option<Diagnostic> {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Diagnostic::new(
                violations::DefaultExceptNotLast,
                except_range(handler, locator),
            ));
        }
    }

    None
}

/// F621, F622
pub fn starred_expressions(
    elts: &[Expr],
    check_too_many_expressions: bool,
    check_two_starred_expressions: bool,
    location: Range,
) -> Option<Diagnostic> {
    let mut has_starred: bool = false;
    let mut starred_index: Option<usize> = None;
    for (index, elt) in elts.iter().enumerate() {
        if matches!(elt.node, ExprKind::Starred { .. }) {
            if has_starred && check_two_starred_expressions {
                return Some(Diagnostic::new(violations::TwoStarredExpressions, location));
            }
            has_starred = true;
            starred_index = Some(index);
        }
    }

    if check_too_many_expressions {
        if let Some(starred_index) = starred_index {
            if starred_index >= 1 << 8 || elts.len() - starred_index > 1 << 24 {
                return Some(Diagnostic::new(
                    violations::ExpressionsInStarAssignment,
                    location,
                ));
            }
        }
    }

    None
}

/// F701
pub fn break_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut allowed: bool = false;
    let mut child = stmt;
    for parent in parents {
        match &parent.node {
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    if allowed {
        None
    } else {
        Some(Diagnostic::new(
            violations::BreakOutsideLoop,
            Range::from_located(stmt),
        ))
    }
}

/// F702
pub fn continue_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut allowed: bool = false;
    let mut child = stmt;
    for parent in parents {
        match &parent.node {
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    if allowed {
        None
    } else {
        Some(Diagnostic::new(
            violations::ContinueOutsideLoop,
            Range::from_located(stmt),
        ))
    }
}
