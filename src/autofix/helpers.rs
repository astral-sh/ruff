use crate::ast::helpers::to_absolute;
use crate::ast::types::Range;
use anyhow::{bail, Result};
use itertools::Itertools;
use rustpython_parser::ast::{ExcepthandlerKind, Location, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::autofix::Fix;
use crate::source_code_locator::SourceCodeLocator;

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
                bail!("Unable to find child in parent body")
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
                bail!("Unable to find child in parent body")
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
                bail!("Unable to find child in parent body")
            }
        }
        _ => bail!("Unable to find child in parent body"),
    }
}

// The algorithm should be: keep skipping lines that "start" with whitespace or a backslash or a hash.
// Keep going until we find the first character after a semi.
fn removal_range(locator: &SourceCodeLocator, stmt: &Stmt) -> Range {
    // Keep going until we see something that isn't a newline, or a semicolon.
    let contents = locator.slice_source_code_at(&stmt.end_location.unwrap());
    let mut last = None;
    for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
        last = Some(end);
        if matches!(tok, Tok::Newline | Tok::Semi) {
            continue;
        }
        return Range {
            location: stmt.location,
            end_location: to_absolute(start, stmt.end_location.unwrap()),
        };
    }
    if let Some(last) = last {
        Range {
            location: stmt.location,
            end_location: to_absolute(last, stmt.end_location.unwrap()),
        }
    } else {
        Range {
            location: stmt.location,
            end_location: stmt.end_location.unwrap(),
        }
    }
}

pub fn remove_stmt(
    locator: &SourceCodeLocator,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let range = removal_range(locator, stmt);
    if parent
        .map(|parent| is_lone_child(stmt, parent, deleted))
        .map_or(Ok(None), |v| v.map(Some))?
        .unwrap_or_default()
    {
        // If removing this node would lead to an invalid syntax tree, replace
        // it with a `pass`.
        println!("Replacing with pass: {:?}", range);
        Ok(Fix::replacement(
            "pass".to_string(),
            range.location,
            range.end_location,
        ))
    } else {
        println!("Deleting: {:?}", range);
        Ok(Fix::deletion(range.location, range.end_location))
    }
}
