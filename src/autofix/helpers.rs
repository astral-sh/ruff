use anyhow::{bail, Result};
use itertools::Itertools;
use rustpython_parser::ast::{ExcepthandlerKind, Location, Stmt, StmtKind};

use crate::ast::helpers;
use crate::ast::helpers::to_absolute;
use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
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

/// Return the location of a trailing semicolon following a `Stmt`, if it's part
/// of a multi-statement line.
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
                Location::new(row + 1, column),
                stmt.end_location.unwrap(),
            ));
        }
        if !trimmed.starts_with('\\') {
            break;
        }
    }
    None
}

/// Find the start of the next `Stmt` after a semicolon.
fn next_valid_character(semicolon: Location, locator: &SourceCodeLocator) -> Location {
    let start_location = Location::new(semicolon.row(), semicolon.column() + 1);
    let contents = locator.slice_source_code_at(&start_location);
    for (row, line) in LinesWithTrailingNewline::from(&contents).enumerate() {
        let trimmed = line.trim();
        // Skip past any continuations.
        if trimmed.starts_with('\\') {
            continue;
        }
        return if trimmed.is_empty() {
            // If the line is empty, then despite the previous statement ending in a
            // semicolon, we know that it's not a multi-statement line.
            to_absolute(Location::new(row + 1, 0), start_location)
        } else {
            // Otherwise, find the start of the next statement. (Or, anything that isn't
            // whitespace.)
            let column = line
                .char_indices()
                .find_map(|(column, char)| {
                    if char.is_whitespace() {
                        None
                    } else {
                        Some(column)
                    }
                })
                .unwrap();
            to_absolute(Location::new(row + 1, column), start_location)
        };
    }
    Location::new(start_location.row() + 1, 0)
}

/// Return the `Range` to use when deleting a `Stmt`.
///
/// In some cases, this is as simple as the `Range` of the `Stmt` itself.
/// However, there are a few exceptions:
/// - If the `Stmt` has no trailing and leading content, then it's convenient to
///   remove the entire start and end lines.
/// - If the `Stmt` is _not_ the terminal statement in a multi-statement line,
///   we need to delete up to the start of the next statement (and avoid
///   deleting any content that precedes the statement).
/// - If the `Stmt` is the terminal statement in a multi-statement line, we need
///   to avoid deleting any content that precedes the statement.
fn deletion_range(stmt: &Stmt, locator: &SourceCodeLocator) -> Range {
    if let Some(semicolon) = trailing_semicolon(locator, stmt) {
        let next = next_valid_character(semicolon, locator);
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

pub fn delete_stmt(
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
    locator: &SourceCodeLocator,
) -> Result<Fix> {
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
        let range = deletion_range(stmt, locator);
        Ok(Fix::deletion(range.location, range.end_location))
    }
}
