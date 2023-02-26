use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::source_code::Stylist;
use rustpython_parser::Tok;

pub fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

pub fn compare(left: &Expr, ops: &[Cmpop], comparators: &[Expr], stylist: &Stylist) -> String {
    unparse_expr(
        &create_expr(ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        }),
        stylist,
    )
}

static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://\S+$").unwrap());

pub fn is_overlong(
    line: &str,
    line_length: usize,
    limit: usize,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
) -> bool {
    if line_length <= limit {
        return false;
    }

    let mut chunks = line.split_whitespace();
    let (Some(first), Some(second)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return false;
    };

    if first == "#" {
        if ignore_overlong_task_comments {
            let second = second.trim_end_matches(':');
            if task_tags.iter().any(|tag| tag == second) {
                return false;
            }
        }

        // Do not enforce the line length for commented lines that end with a URL
        // or contain only a single word.
        if chunks.last().map_or(true, |c| URL_REGEX.is_match(c)) {
            return false;
        }
    }

    true
}

pub fn is_keyword_token(token: &Tok) -> bool {
    match token {
        Tok::False { .. } => true,
        Tok::True { .. } => true,
        Tok::None { .. } => true,
        Tok::And { .. } => true,
        Tok::As { .. } => true,
        Tok::Assert { .. } => true,
        Tok::Await { .. } => true,
        Tok::Break { .. } => true,
        Tok::Class { .. } => true,
        Tok::Continue { .. } => true,
        Tok::Def { .. } => true,
        Tok::Del { .. } => true,
        Tok::Elif { .. } => true,
        Tok::Else { .. } => true,
        Tok::Except { .. } => true,
        Tok::Finally { .. } => true,
        Tok::For { .. } => true,
        Tok::From { .. } => true,
        Tok::Global { .. } => true,
        Tok::If { .. } => true,
        Tok::Import { .. } => true,
        Tok::In { .. } => true,
        Tok::Is { .. } => true,
        Tok::Lambda { .. } => true,
        Tok::Nonlocal { .. } => true,
        Tok::Not { .. } => true,
        Tok::Or { .. } => true,
        Tok::Pass { .. } => true,
        Tok::Raise { .. } => true,
        Tok::Return { .. } => true,
        Tok::Try { .. } => true,
        Tok::While { .. } => true,
        Tok::With { .. } => true,
        Tok::Yield { .. } => true,
        _ => false,
    }
}

pub fn is_singleton_token(token: &Tok) -> bool {
    match token {
        Tok::False { .. } => true,
        Tok::True { .. } => true,
        Tok::None { .. } => true,
        _ => false,
    }
}
