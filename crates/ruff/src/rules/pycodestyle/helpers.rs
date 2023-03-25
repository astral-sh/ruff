use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};
use rustpython_parser::Tok;

use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::token_kind::TokenKind;

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
    line_width: usize,
    limit: usize,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
) -> bool {
    if line_width <= limit {
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

pub const fn is_keyword_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_keyword()
}

pub const fn is_singleton_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_singleton()
}

pub const fn is_op_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_operator()
}

pub const fn is_skip_comment_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_skip_comment()
}

pub const fn is_soft_keyword_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_soft_keyword()
}

pub const fn is_arithmetic_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_arithmetic()
}

pub const fn is_ws_optional_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_whitespace_optional()
}

pub const fn is_ws_needed_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_whitespace_needed()
}

pub const fn is_unary_token(token: &Tok) -> bool {
    TokenKind::from_token(token).is_unary()
}
