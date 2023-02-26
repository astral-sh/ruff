use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};
use rustpython_parser::Tok;

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::source_code::Stylist;

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
    matches!(
        token,
        Tok::False { .. }
            | Tok::True { .. }
            | Tok::None { .. }
            | Tok::And { .. }
            | Tok::As { .. }
            | Tok::Assert { .. }
            | Tok::Await { .. }
            | Tok::Break { .. }
            | Tok::Class { .. }
            | Tok::Continue { .. }
            | Tok::Def { .. }
            | Tok::Del { .. }
            | Tok::Elif { .. }
            | Tok::Else { .. }
            | Tok::Except { .. }
            | Tok::Finally { .. }
            | Tok::For { .. }
            | Tok::From { .. }
            | Tok::Global { .. }
            | Tok::If { .. }
            | Tok::Import { .. }
            | Tok::In { .. }
            | Tok::Is { .. }
            | Tok::Lambda { .. }
            | Tok::Nonlocal { .. }
            | Tok::Not { .. }
            | Tok::Or { .. }
            | Tok::Pass { .. }
            | Tok::Raise { .. }
            | Tok::Return { .. }
            | Tok::Try { .. }
            | Tok::While { .. }
            | Tok::With { .. }
            | Tok::Yield { .. }
    )
}

pub fn is_singleton_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::False { .. } | Tok::True { .. } | Tok::None { .. },
    )
}
