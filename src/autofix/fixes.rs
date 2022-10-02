use itertools::Itertools;
use libcst_native::ImportNames::Aliases;
use libcst_native::NameOrAttribute::N;
use libcst_native::{Codegen, Expression, SmallStatement, Statement};
use rustpython_parser::ast::{Expr, Keyword, Location, Stmt, StmtKind};
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

fn is_lone_child(parent: &Stmt, child: &Stmt) -> bool {
    match &parent.node {
        StmtKind::FunctionDef { body, .. }
        | StmtKind::AsyncFunctionDef { body, .. }
        | StmtKind::ClassDef { body, .. }
        | StmtKind::With { body, .. }
        | StmtKind::AsyncWith { body, .. } => body.len() == 1,
        StmtKind::For { body, orelse, .. }
        | StmtKind::AsyncFor { body, orelse, .. }
        | StmtKind::While { body, orelse, .. }
        | StmtKind::If { body, orelse, .. } => {
            if body.iter().contains(child) {
                body.len() == 1
            } else if orelse.iter().contains(child) {
                orelse.len() == 1
            } else {
                false
            }
        }
        StmtKind::Try {
            body,
            orelse,
            finalbody,
            ..
        } => {
            if body.iter().contains(child) {
                body.len() == 1
            } else if orelse.iter().contains(child) {
                orelse.len() == 1
            } else if finalbody.iter().contains(child) {
                finalbody.len() == 1
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn remove_unused_imports(
    locator: &mut SourceCodeLocator,
    full_names: &[String],
    stmt: &Stmt,
    parent: Option<&Stmt>,
) -> Option<Fix> {
    let contents = locator.slice_source_code_range(&Range::from_located(stmt));

    let location = stmt.location;
    let end_location = stmt.end_location;

    // TODO(charlie): Not necessary if we're removing a non-`from`, so just track that ahead of
    // time.
    let mut tree = match libcst_native::parse_module(contents, None) {
        Ok(m) => m,
        Err(_) => return None,
    };

    // import collections
    if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        if let Some(SmallStatement::Import(_)) = body.body.first_mut() {
            if parent
                .map(|parent| is_lone_child(parent, stmt))
                .unwrap_or_default()
            {
                return Some(Fix {
                    location,
                    end_location,
                    content: "pass".to_string(),
                    applied: false,
                });
            }

            let suffix = locator.slice_source_code_at(&location);
            let mut adjusted_end_location = end_location;
            for (start, tok, end) in lexer::make_tokenizer(suffix).flatten() {
                if to_absolute(&end, &location) <= end_location {
                    continue;
                }
                if matches!(tok, Tok::Semi) {
                    continue;
                }
                if matches!(tok, Tok::Newline) {
                    adjusted_end_location = to_absolute(&end, &location);
                } else {
                    adjusted_end_location = to_absolute(&start, &location);
                }
                break;
            }

            return Some(Fix {
                location,
                end_location: adjusted_end_location,
                content: "".to_string(),
                applied: false,
            });
        }
    }

    // from collections import OrderedDict
    if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        if let Some(SmallStatement::ImportFrom(body)) = body.body.first_mut() {
            if let Aliases(aliases) = &mut body.names {
                // Preserve the trailing comma (or not) from the last entry.
                let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

                let mut removable = vec![];
                for (index, alias) in aliases.iter().enumerate() {
                    if let N(name) = &alias.name {
                        let import_name = if let Some(N(module_name)) = &body.module {
                            format!("{}.{}", module_name.value, name.value)
                        } else {
                            name.value.to_string()
                        };
                        if full_names.contains(&import_name) {
                            removable.push(index);
                        }
                    }
                }
                for index in removable.iter() {
                    aliases.remove(*index);
                }

                if let Some(alias) = aliases.last_mut() {
                    alias.comma = trailing_comma;
                }

                return if aliases.is_empty() {
                    if parent
                        .map(|parent| is_lone_child(parent, stmt))
                        .unwrap_or_default()
                    {
                        return Some(Fix {
                            location,
                            end_location,
                            content: "pass".to_string(),
                            applied: false,
                        });
                    }

                    let suffix = locator.slice_source_code_at(&location);
                    let mut adjusted_end_location = end_location;
                    for (start, tok, end) in lexer::make_tokenizer(suffix).flatten() {
                        if to_absolute(&end, &location) <= end_location {
                            continue;
                        }
                        if matches!(tok, Tok::Semi) {
                            continue;
                        }
                        if matches!(tok, Tok::Newline) {
                            adjusted_end_location = to_absolute(&end, &location);
                        } else {
                            adjusted_end_location = to_absolute(&start, &location);
                        }
                        break;
                    }
                    Some(Fix {
                        location,
                        end_location: adjusted_end_location,
                        content: "".to_string(),
                        applied: false,
                    })
                } else {
                    let mut state = Default::default();
                    tree.codegen(&mut state);

                    Some(Fix {
                        content: state.to_string(),
                        location,
                        end_location,
                        applied: false,
                    })
                };
            }
        }
    }

    None
}
