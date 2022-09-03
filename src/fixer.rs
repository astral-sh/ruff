use rustpython_parser::ast::{Expr, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::checks::Fix;

fn to_absolute(location: &Location, base: &Location) -> Location {
    if location.row() == 1 {
        Location::new(
            location.row() + base.row() - 1,
            location.column() + base.column() - 1,
        )
    } else {
        Location::new(location.row() + base.row() - 1, location.column())
    }
}

pub fn remove_object_base(
    lines: &[&str],
    stmt_at: &Location,
    expr_at: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    // Case 1: `object` is the only base.
    if bases.len() == 1 && keywords.is_empty() {
        let lxr = lexer::make_tokenizer(&lines[stmt_at.row() - 1][stmt_at.column() - 1..]);
        let mut fix_start = None;
        let mut fix_end = None;
        let mut count: usize = 0;
        for (start, tok, end) in lxr.flatten() {
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
                }
            }

            if fix_start.is_some() && fix_end.is_some() {
                break;
            };
        }

        return Some(Fix {
            content: "".to_string(),
            start: fix_start.unwrap(),
            end: fix_end.unwrap(),
        });
    }

    // Case 2: `object` is _not_ the last node.
    let mut closest_after_expr: Option<Location> = None;
    for location in bases
        .iter()
        .map(|node| node.location)
        .chain(keywords.iter().map(|node| node.location))
    {
        // If the node comes after the node we're removing...
        if location.row() > expr_at.row()
            || (location.row() == expr_at.row() && location.column() > expr_at.column())
        {
            match closest_after_expr {
                None => closest_after_expr = Some(location),
                Some(existing) => {
                    // And before the next closest node...
                    if location.row() < existing.row()
                        || (location.row() == existing.row()
                            && location.column() < existing.column())
                    {
                        closest_after_expr = Some(location);
                    }
                }
            };
        }
    }

    match closest_after_expr {
        Some(end) => {
            return Some(Fix {
                content: "".to_string(),
                start: expr_at,
                end,
            });
        }
        None => {}
    }

    // Case 3: `object` is the last node, so we have to find the last token that isn't a comma.
    let lxr = lexer::make_tokenizer(&lines[stmt_at.row() - 1][stmt_at.column() - 1..]);
    let mut fix_start: Option<Location> = None;
    let mut fix_end: Option<Location> = None;
    for (start, tok, end) in lxr.flatten() {
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
        }),
        _ => None,
    }
}
