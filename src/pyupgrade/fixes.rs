use anyhow::Result;
use libcst_native::{Codegen, Expression, ImportNames, SmallStatement, Statement};
use rustpython_ast::{Expr, Keyword, Location, Stmt};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::autofix::{self, Fix};
use crate::cst::matchers::match_module;
use crate::source_code_locator::SourceCodeLocator;

/// Generate a fix to remove a base from a ClassDef statement.
pub fn remove_class_def_base(
    locator: &SourceCodeLocator,
    stmt_at: &Location,
    expr_at: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    let contents = locator.slice_source_code_at(stmt_at);

    // Case 1: `object` is the only base.
    if bases.len() == 1 && keywords.is_empty() {
        let mut fix_start = None;
        let mut fix_end = None;
        let mut count: usize = 0;
        for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(helpers::to_absolute(&start, stmt_at));
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(helpers::to_absolute(&end, stmt_at));
                    break;
                }
            }
        }

        return match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix::deletion(start, end)),
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
        for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
            let start = helpers::to_absolute(&start, stmt_at);
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
            (Some(start), Some(end)) => Some(Fix::replacement("".to_string(), start, end)),
            _ => None,
        }
    } else {
        // Case 3: `object` is the last node, so we have to find the last token that
        // isn't a comma.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
            let start = helpers::to_absolute(&start, stmt_at);
            let end = helpers::to_absolute(&end, stmt_at);
            if start == expr_at {
                fix_end = Some(end);
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(start);
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix::replacement("".to_string(), start, end)),
            _ => None,
        }
    }
}

pub fn remove_super_arguments(locator: &SourceCodeLocator, expr: &Expr) -> Option<Fix> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut tree = match libcst_native::parse_module(&contents, None) {
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

                return Some(Fix::replacement(
                    state.to_string(),
                    range.location,
                    range.end_location,
                ));
            }
        }
    }

    None
}

/// U010
pub fn remove_unnecessary_future_import(
    locator: &SourceCodeLocator,
    removable: &[usize],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    // TODO(charlie): DRY up with pyflakes::fixes::remove_unused_import_froms.
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text)?;

    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple"));
    };
    let body = if let Some(SmallStatement::ImportFrom(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::ImportFrom"
        ));
    };

    let aliases = if let ImportNames::Aliases(aliases) = &mut body.names {
        aliases
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Aliases"));
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // TODO(charlie): This is quadratic.
    for index in removable.iter().rev() {
        aliases.remove(*index);
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        autofix::helpers::remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = Default::default();
        tree.codegen(&mut state);

        Ok(Fix::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}
