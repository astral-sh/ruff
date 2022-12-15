use crate::ast::helpers;
use crate::ast::helpers::to_absolute;
use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
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

/// Return the location of a trailing semicolon following a `Stmt`, if it's part of a multi-statement line.
fn trailing_semicolon(locator: &SourceCodeLocator, stmt: &Stmt) -> Option<Location> {
    let contents = locator.slice_source_code_at(&stmt.end_location.unwrap());
    for (row, line) in LinesWithTrailingNewline::from(&contents).enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(';') {
            let column = line
                .char_indices()
                .find_map(|(column, char)| if char == ';' { Some(column) } else { None })
                .unwrap();
            return Some(to_absolute(
                Location::new(row, column),
                stmt.end_location.unwrap(),
            ));
        }
        if !trimmed.starts_with('\\') {
            break;
        }
    }
    None
}

fn next_valid_character(locator: &SourceCodeLocator, semicolon: Location) -> Location {
    let contents =
        locator.slice_source_code_at(&Location::new(semicolon.row(), semicolon.column() + 1));
    for (row, line) in LinesWithTrailingNewline::from(&contents).enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('\\') {
            continue;
        }
        return if trimmed.is_empty() {
            to_absolute(Location::new(row + 1, 0), semicolon)
        } else {
            let column = line
                .char_indices()
                .find_map(|(column, char)| {
                    if !char.is_whitespace() {
                        Some(column)
                    } else {
                        None
                    }
                })
                .unwrap();
            to_absolute(Location::new(row, column), semicolon)
        };
    }
    Location::new(semicolon.row() + 1, 0)
}

// The algorithm should be: keep skipping lines that "start" with whitespace or a backslash or a hash.
// Keep going until we find the first character after a semi.
fn removal_range(locator: &SourceCodeLocator, stmt: &Stmt) -> Range {
    if let Some(semicolon) = trailing_semicolon(locator, stmt) {
        let next = next_valid_character(locator, semicolon);
        Range {
            location: stmt.location,
            end_location: next,
        }
    } else if helpers::match_leading_content(stmt, locator) {
        Range::from_located(stmt)
    } else {
        Range {
            location: Location::new(stmt.location.row(), 0),
            end_location: Location::new(stmt.end_location.unwrap().row() + 1, 0),
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
