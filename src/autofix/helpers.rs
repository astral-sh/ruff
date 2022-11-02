use anyhow::Result;
use itertools::Itertools;
use rustpython_parser::ast::{ExcepthandlerKind, Location, Stmt, StmtKind};

use crate::autofix::Fix;

/// Determine if a body contains only a single statement, taking into account
/// deleted.
fn has_single_child(body: &[Stmt], deleted: &[&Stmt]) -> bool {
    body.iter().filter(|child| !deleted.contains(child)).count() == 1
}

/// Determine if a child is the only statement in its body.
fn is_lone_child(child: &Stmt, parent: &Stmt, deleted: &[&Stmt]) -> Result<bool> {
    match &parent.node {
        StmtKind::FunctionDef { body, .. }
        | StmtKind::AsyncFunctionDef { body, .. }
        | StmtKind::ClassDef { body, .. }
        | StmtKind::With { body, .. }
        | StmtKind::AsyncWith { body, .. } => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else {
                Err(anyhow::anyhow!("Unable to find child in parent body."))
            }
        }
        StmtKind::For { body, orelse, .. }
        | StmtKind::AsyncFor { body, orelse, .. }
        | StmtKind::While { body, orelse, .. }
        | StmtKind::If { body, orelse, .. } => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else if orelse.iter().contains(child) {
                Ok(has_single_child(orelse, deleted))
            } else {
                Err(anyhow::anyhow!("Unable to find child in parent body."))
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            if body.iter().contains(child) {
                Ok(has_single_child(body, deleted))
            } else if orelse.iter().contains(child) {
                Ok(has_single_child(orelse, deleted))
            } else if finalbody.iter().contains(child) {
                Ok(has_single_child(finalbody, deleted))
            } else if let Some(body) = handlers.iter().find_map(|handler| match &handler.node {
                ExcepthandlerKind::ExceptHandler { body, .. } => {
                    if body.iter().contains(child) {
                        Some(body)
                    } else {
                        None
                    }
                }
            }) {
                Ok(has_single_child(body, deleted))
            } else {
                Err(anyhow::anyhow!("Unable to find child in parent body."))
            }
        }
        _ => Err(anyhow::anyhow!("Unable to find child in parent body.")),
    }
}

pub fn remove_stmt(stmt: &Stmt, parent: Option<&Stmt>, deleted: &[&Stmt]) -> Result<Fix> {
    if parent
        .map(|parent| is_lone_child(stmt, parent, deleted))
        .map_or(Ok(None), |v| v.map(Some))?
        .unwrap_or_default()
    {
        // If removing this node would lead to an invalid syntax tree, replace
        // it with a `pass`.
        Ok(Fix::replacement(
            "pass".to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    } else {
        // Otherwise, nuke the entire line.
        // TODO(charlie): This logic assumes that there are no multi-statement physical
        // lines.
        Ok(Fix::deletion(
            Location::new(stmt.location.row(), 0),
            Location::new(stmt.end_location.unwrap().row() + 1, 0),
        ))
    }
}
