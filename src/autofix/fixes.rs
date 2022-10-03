use anyhow::Result;
use itertools::Itertools;
use libcst_native::ImportNames::Aliases;
use libcst_native::NameOrAttribute::N;
use libcst_native::{Codegen, Expression, SmallStatement, Statement};
use rustpython_parser::ast::{ExcepthandlerKind, Expr, Keyword, Location, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::operations::SourceCodeLocator;
use crate::ast::types::Range;
use crate::checks::Fix;

/// Convert a location within a file (relative to `base`) to an absolute position.
fn to_absolute(relative: &Location, base: &Location) -> Location {
    if relative.row() == 1 {
        Location::new(
            relative.row() + base.row() - 1,
            relative.column() + base.column() - 1,
        )
    } else {
        Location::new(relative.row() + base.row() - 1, relative.column())
    }
}

/// Generate a fix to remove a base from a ClassDef statement.
pub fn remove_class_def_base(
    locator: &mut SourceCodeLocator,
    stmt_at: &Location,
    expr_at: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    let content = locator.slice_source_code_at(stmt_at);

    // Case 1: `object` is the only base.
    if bases.len() == 1 && keywords.is_empty() {
        let mut fix_start = None;
        let mut fix_end = None;
        let mut count: usize = 0;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(to_absolute(&start, stmt_at));
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(to_absolute(&end, stmt_at));
                    break;
                }
            }
        }

        return match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        };
    }

    if bases
        .iter()
        .map(|node| node.location)
        .chain(keywords.iter().map(|node| node.location))
        .any(|location| location > expr_at)
    {
        // Case 2: `object` is _not_ the last node.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        let mut seen_comma = false;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            let start = to_absolute(&start, stmt_at);
            if seen_comma {
                if matches!(tok, Tok::Newline) {
                    fix_end = Some(end);
                } else {
                    fix_end = Some(start);
                }
                break;
            }
            if start == expr_at {
                fix_start = Some(start);
            }
            if fix_start.is_some() && matches!(tok, Tok::Comma) {
                seen_comma = true;
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        }
    } else {
        // Case 3: `object` is the last node, so we have to find the last token that isn't a comma.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            let start = to_absolute(&start, stmt_at);
            let end = to_absolute(&end, stmt_at);
            if start == expr_at {
                fix_end = Some(end);
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(start);
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        }
    }
}

pub fn remove_super_arguments(locator: &mut SourceCodeLocator, expr: &Expr) -> Option<Fix> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut tree = match libcst_native::parse_module(contents, None) {
        Ok(m) => m,
        Err(_) => return None,
    };

    if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
            if let Expression::Call(body) = &mut body.value {
                body.args = vec![];
                body.whitespace_before_args = Default::default();
                body.whitespace_after_func = Default::default();

                let mut state = Default::default();
                tree.codegen(&mut state);

                return Some(Fix {
                    content: state.to_string(),
                    location: range.location,
                    end_location: range.end_location,
                    applied: false,
                });
            }
        }
    }

    None
}

/// Determine if a body contains only a single statement, taking into account deleted.
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
        Ok(Fix {
            location: stmt.location,
            end_location: stmt.end_location,
            content: "pass".to_string(),
            applied: false,
        })
    } else {
        // Otherwise, nuke the entire line.
        // TODO(charlie): This logic assumes that there are no multi-statement physical lines.
        Ok(Fix {
            location: Location::new(stmt.location.row(), 1),
            end_location: Location::new(stmt.end_location.row() + 1, 1),
            content: "".to_string(),
            applied: false,
        })
    }
}

/// Generate a Fix to remove any unused imports from an `import` statement.
pub fn remove_unused_imports(
    locator: &mut SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(stmt)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };

    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Import(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::ImportFrom."
        ));
    };
    let aliases = &mut body.names;

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // Identify unused imports from within the `import from`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if let N(import_name) = &alias.name {
            if full_names.contains(&import_name.value) {
                removable.push(index);
            }
        }
    }
    // TODO(charlie): This is quadratic.
    for index in removable.iter().rev() {
        aliases.remove(*index);
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = Default::default();
        tree.codegen(&mut state);

        Ok(Fix {
            content: state.to_string(),
            location: stmt.location,
            end_location: stmt.end_location,
            applied: false,
        })
    }
}

/// Generate a Fix to remove any unused imports from an `import from` statement.
pub fn remove_unused_import_froms(
    locator: &mut SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(stmt)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };

    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::ImportFrom(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::ImportFrom."
        ));
    };
    let aliases = if let Aliases(aliases) = &mut body.names {
        aliases
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Aliases."));
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // Identify unused imports from within the `import from`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if let N(name) = &alias.name {
            let import_name = if let Some(N(module_name)) = &body.module {
                format!("{}.{}", module_name.value, name.value)
            } else {
                name.value.to_string()
            };
            if full_names.contains(&import_name.as_str()) {
                removable.push(index);
            }
        }
    }
    // TODO(charlie): This is quadratic.
    for index in removable.iter().rev() {
        aliases.remove(*index);
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = Default::default();
        tree.codegen(&mut state);

        Ok(Fix {
            content: state.to_string(),
            location: stmt.location,
            end_location: stmt.end_location,
            applied: false,
        })
    }
}
