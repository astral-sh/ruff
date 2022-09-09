use rustpython_parser::ast::{Expr, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::operations::SourceCodeLocator;
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
    let content = locator.slice_source_code(stmt_at);

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
                start,
                end,
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
                start,
                end,
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
                start,
                end,
                applied: false,
            }),
            _ => None,
        }
    }
}
