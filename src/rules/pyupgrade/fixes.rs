use libcst_native::{
    Codegen, CodegenState, Expression, ParenthesizableWhitespace, SmallStatement, Statement,
};
use rustpython_ast::{Expr, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// Generate a fix to remove a base from a `ClassDef` statement.
pub fn remove_class_def_base(
    locator: &Locator,
    stmt_at: Location,
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
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(start);
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(end);
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
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if seen_comma {
                if matches!(tok, Tok::NonLogicalNewline) {
                    // Also delete any non-logical newlines after the comma.
                    continue;
                }
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
            (Some(start), Some(end)) => Some(Fix::replacement(String::new(), start, end)),
            _ => None,
        }
    } else {
        // Case 3: `object` is the last node, so we have to find the last token that
        // isn't a comma.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt_at).flatten() {
            if start == expr_at {
                fix_end = Some(end);
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(start);
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix::replacement(String::new(), start, end)),
            _ => None,
        }
    }
}

/// Generate a fix to remove arguments from a `super` call.
pub fn remove_super_arguments(locator: &Locator, stylist: &Stylist, expr: &Expr) -> Option<Fix> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut tree = libcst_native::parse_module(contents, None).ok()?;

    let Statement::Simple(body) = tree.body.first_mut()? else {
        return None;
    };
    let SmallStatement::Expr(body) = body.body.first_mut()? else {
        return None;
    };
    let Expression::Call(body) = &mut body.value else {
        return None;
    };

    body.args = vec![];
    body.whitespace_before_args = ParenthesizableWhitespace::default();
    body.whitespace_after_func = ParenthesizableWhitespace::default();

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Some(Fix::replacement(
        state.to_string(),
        range.location,
        range.end_location,
    ))
}
