use anyhow::{bail, Result};
use itertools::Itertools;
use libcst_native::{
    Codegen, CodegenState, ImportNames, ParenthesizableWhitespace, SmallStatement, Statement,
};
use rustpython_parser::ast::{ExcepthandlerKind, Expr, Keyword, Location, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::helpers;
use crate::ast::helpers::to_absolute;
use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::cst::helpers::compose_module_path;
use crate::cst::matchers::match_module;
use crate::fix::Fix;
use crate::source_code::{Indexer, Locator, Stylist};

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
fn trailing_semicolon(stmt: &Stmt, locator: &Locator) -> Option<Location> {
    let contents = locator.slice_source_code_at(stmt.end_location.unwrap());
    for (row, line) in LinesWithTrailingNewline::from(contents).enumerate() {
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
fn next_stmt_break(semicolon: Location, locator: &Locator) -> Location {
    let start_location = Location::new(semicolon.row(), semicolon.column() + 1);
    let contents = locator.slice_source_code_at(start_location);
    for (row, line) in LinesWithTrailingNewline::from(contents).enumerate() {
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
fn is_end_of_file(stmt: &Stmt, locator: &Locator) -> bool {
    let contents = locator.slice_source_code_at(stmt.end_location.unwrap());
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
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
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
        } else if helpers::preceded_by_continuation(stmt, indexer) {
            if is_end_of_file(stmt, locator) && stmt.location.column() == 0 {
                // Special-case: a file can't end in a continuation.
                Fix::replacement(
                    stylist.line_ending().to_string(),
                    stmt.location,
                    stmt.end_location.unwrap(),
                )
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

/// Generate a `Fix` to remove any unused imports from an `import` statement.
pub fn remove_unused_imports<'a>(
    unused_imports: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(module_text)?;

    let Some(Statement::Simple(body)) = tree.body.first_mut() else {
        bail!("Expected Statement::Simple");
    };

    let (aliases, import_module) = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => (&mut import_body.names, None),
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                (
                    names,
                    Some((&import_body.relative, import_body.module.as_ref())),
                )
            } else if let ImportNames::Star(..) = &import_body.names {
                // Special-case: if the import is a `from ... import *`, then we delete the
                // entire statement.
                let mut found_star = false;
                for unused_import in unused_imports {
                    let full_name = match import_body.module.as_ref() {
                        Some(module_name) => format!("{}.*", compose_module_path(module_name)),
                        None => "*".to_string(),
                    };
                    if unused_import == full_name {
                        found_star = true;
                    } else {
                        bail!(
                            "Expected \"*\" for unused import (got: \"{}\")",
                            unused_import
                        );
                    }
                }
                if !found_star {
                    bail!("Expected \'*\' for unused import");
                }
                return delete_stmt(stmt, parent, deleted, locator, indexer, stylist);
            } else {
                bail!("Expected: ImportNames::Aliases | ImportNames::Star");
            }
        }
        _ => bail!("Expected: SmallStatement::ImportFrom | SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    for unused_import in unused_imports {
        let alias_index = aliases.iter().position(|alias| {
            let full_name = match import_module {
                Some((relative, module)) => {
                    let module = module.map(compose_module_path);
                    let member = compose_module_path(&alias.name);
                    let mut full_name = String::with_capacity(
                        relative.len()
                            + module.as_ref().map_or(0, std::string::String::len)
                            + member.len()
                            + 1,
                    );
                    for _ in 0..relative.len() {
                        full_name.push('.');
                    }
                    if let Some(module) = module {
                        full_name.push_str(&module);
                        full_name.push('.');
                    }
                    full_name.push_str(&member);
                    full_name
                }
                None => compose_module_path(&alias.name),
            };
            full_name == unused_import
        });

        if let Some(index) = alias_index {
            aliases.remove(index);
        }
    }

    // But avoid destroying any trailing comments.
    if let Some(alias) = aliases.last_mut() {
        let has_comment = if let Some(comma) = &alias.comma {
            match &comma.whitespace_after {
                ParenthesizableWhitespace::SimpleWhitespace(_) => false,
                ParenthesizableWhitespace::ParenthesizedWhitespace(whitespace) => {
                    whitespace.first_line.comment.is_some()
                }
            }
        } else {
            false
        };
        if !has_comment {
            alias.comma = trailing_comma;
        }
    }

    if aliases.is_empty() {
        delete_stmt(stmt, parent, deleted, locator, indexer, stylist)
    } else {
        let mut state = CodegenState {
            default_newline: stylist.line_ending(),
            default_indent: stylist.indentation(),
            ..CodegenState::default()
        };
        tree.codegen(&mut state);

        Ok(Fix::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}

/// Generic function to remove arguments or keyword arguments in function
/// calls and class definitions. (For classes `args` should be considered
/// `bases`)
///
/// Supports the removal of parentheses when this is the only (kw)arg left.
/// For this behavior, set `remove_parentheses` to `true`.
pub fn remove_argument(
    locator: &Locator,
    stmt_at: Location,
    expr_at: Location,
    expr_end: Location,
    args: &[Expr],
    keywords: &[Keyword],
    remove_parentheses: bool,
) -> Result<Fix> {
    // TODO(sbrugman): Preserve trailing comments.
    let contents = locator.slice_source_code_at(stmt_at);

    let mut fix_start = None;
    let mut fix_end = None;

    let n_arguments = keywords.len() + args.len();
    if n_arguments == 0 {
        bail!("No arguments or keywords to remove");
    }

    if n_arguments == 1 {
        // Case 1: there is only one argument.
        let mut count: usize = 0;
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(if remove_parentheses {
                        start
                    } else {
                        Location::new(start.row(), start.column() + 1)
                    });
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(if remove_parentheses {
                        end
                    } else {
                        Location::new(end.row(), end.column() - 1)
                    });
                    break;
                }
            }
        }
    } else if args
        .iter()
        .map(|node| node.location)
        .chain(keywords.iter().map(|node| node.location))
        .any(|location| location > expr_at)
    {
        // Case 2: argument or keyword is _not_ the last node.
        let mut seen_comma = false;
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if seen_comma {
                if matches!(tok, Tok::NonLogicalNewline) {
                    // Also delete any non-logical newlines after the comma.
                    continue;
                }
                fix_end = Some(if matches!(tok, Tok::Newline) {
                    end
                } else {
                    start
                });
                break;
            }
            if start == expr_at {
                fix_start = Some(start);
            }
            if fix_start.is_some() && matches!(tok, Tok::Comma) {
                seen_comma = true;
            }
        }
    } else {
        // Case 3: argument or keyword is the last node, so we have to find the last
        // comma in the stmt.
        for (start, tok, _) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if start == expr_at {
                fix_end = Some(expr_end);
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(start);
            }
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Fix::deletion(start, end)),
        _ => {
            bail!("No fix could be constructed")
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::ast::Location;
    use rustpython_parser::parser;

    use crate::autofix::helpers::{next_stmt_break, trailing_semicolon};
    use crate::source_code::Locator;

    #[test]
    fn find_semicolon() -> Result<()> {
        let contents = "x = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(trailing_semicolon(stmt, &locator), None);

        let contents = "x = 1; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt, &locator),
            Some(Location::new(1, 5))
        );

        let contents = "x = 1 ; y = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
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
        let locator = Locator::new(contents);
        assert_eq!(
            trailing_semicolon(stmt, &locator),
            Some(Location::new(2, 2))
        );

        Ok(())
    }

    #[test]
    fn find_next_stmt_break() {
        let contents = "x = 1; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(1, 4), &locator),
            Location::new(1, 5)
        );

        let contents = "x = 1 ; y = 1";
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(1, 5), &locator),
            Location::new(1, 6)
        );

        let contents = r#"
x = 1 \
  ; y = 1
"#
        .trim();
        let locator = Locator::new(contents);
        assert_eq!(
            next_stmt_break(Location::new(2, 2), &locator),
            Location::new(2, 4)
        );
    }
}
