use anyhow::{bail, Result};
use itertools::Itertools;
use rustpython_parser::ast::{ExcepthandlerKind, Location, Stmt, StmtKind};

use crate::ast::helpers;
use crate::ast::helpers::to_absolute;
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
fn trailing_semicolon(stmt: &Stmt, locator: &SourceCodeLocator) -> Option<Location> {
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

/// Find the next valid break for a `Stmt` after a semicolon.
fn next_stmt_break(semicolon: Location, locator: &SourceCodeLocator) -> Location {
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

/// Return `true` if a `Stmt` occurs at the end of a file.
fn is_end_of_file(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    let contents = locator.slice_source_code_at(&stmt.end_location.unwrap());
    contents.is_empty()
}

/// Return the `Fix` to use when deleting a `Stmt`.
///
/// In some cases, this is as simple as deleting the `Range` of the `Stmt`
/// itself. However, there are a few exceptions:
/// - If the `Stmt` is _not_ the terminal statement in a multi-statement line,
///   we need to delete up to the start of the next statement (and avoid
///   deleting any content that precedes the statement).
/// - If the `Stmt` is the terminal statement in a multi-statement line, we need
///   to avoid deleting any content that precedes the statement.
/// - If the `Stmt` has no trailing and leading content, then it's convenient to
///   remove the entire start and end lines.
/// - If the `Stmt` is the last statement in its parent body, replace it with a
///   `pass` instead.
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
        Ok(if let Some(semicolon) = trailing_semicolon(stmt, locator) {
            let next = next_stmt_break(semicolon, locator);
            Fix::deletion(stmt.location, next)
        } else if helpers::match_leading_content(stmt, locator) {
            Fix::deletion(stmt.location, stmt.end_location.unwrap())
        } else if helpers::preceded_by_continuation(stmt, locator) {
            if is_end_of_file(stmt, locator) && stmt.location.column() == 0 {
                // Special-case: a file can't end in a continuation.
                Fix::replacement("\n".to_string(), stmt.location, stmt.end_location.unwrap())
            } else {
                Fix::deletion(stmt.location, stmt.end_location.unwrap())
            }
        } else {
            Fix::deletion(
                Location::new(stmt.location.row(), 0),
                Location::new(stmt.end_location.unwrap().row() + 1, 0),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_ast::Location;
    use rustpython_parser::parser;

    use crate::autofix::helpers::{next_stmt_break, trailing_semicolon};
    use crate::source_code_locator::SourceCodeLocator;

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), None);

        let contents = "x = 1; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt, &locator),
            Some(Location::new(1, 5))
        );

        let contents = "x = 1 ; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt, &locator),
            Some(Location::new(1, 6))
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt, &locator),
            Some(Location::new(2, 2))
        );

        Ok(())
    }

    #[test]
    fn find_next_stmt_break() {
        let contents = "x = 1; y = 1";
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(1, 4), &locator),
            Location::new(1, 5)
        );

        let contents = "x = 1 ; y = 1";
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(1, 5), &locator),
            Location::new(1, 6)
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(2, 2), &locator),
            Location::new(2, 4)
        );
    }
}
