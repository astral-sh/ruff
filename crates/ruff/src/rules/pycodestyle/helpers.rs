use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};
use rustpython_parser::Tok;

use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::source_code::Stylist;

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
        Tok::False
            | Tok::True
            | Tok::None
            | Tok::And
            | Tok::As
            | Tok::Assert
            | Tok::Await
            | Tok::Break
            | Tok::Class
            | Tok::Continue
            | Tok::Def
            | Tok::Del
            | Tok::Elif
            | Tok::Else
            | Tok::Except
            | Tok::Finally
            | Tok::For
            | Tok::From
            | Tok::Global
            | Tok::If
            | Tok::Import
            | Tok::In
            | Tok::Is
            | Tok::Lambda
            | Tok::Nonlocal
            | Tok::Not
            | Tok::Or
            | Tok::Pass
            | Tok::Raise
            | Tok::Return
            | Tok::Try
            | Tok::While
            | Tok::With
            | Tok::Yield
    )
}

pub fn is_singleton_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::False { .. } | Tok::True { .. } | Tok::None { .. },
    )
}

pub fn is_op_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Lpar
            | Tok::Rpar
            | Tok::Lsqb
            | Tok::Rsqb
            | Tok::Comma
            | Tok::Semi
            | Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::Vbar
            | Tok::Amper
            | Tok::Less
            | Tok::Greater
            | Tok::Equal
            | Tok::Dot
            | Tok::Percent
            | Tok::Lbrace
            | Tok::Rbrace
            | Tok::NotEqual
            | Tok::LessEqual
            | Tok::GreaterEqual
            | Tok::Tilde
            | Tok::CircumFlex
            | Tok::LeftShift
            | Tok::RightShift
            | Tok::DoubleStar
            | Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::PercentEqual
            | Tok::AmperEqual
            | Tok::VbarEqual
            | Tok::CircumflexEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::DoubleStarEqual
            | Tok::DoubleSlash
            | Tok::DoubleSlashEqual
            | Tok::At
            | Tok::AtEqual
            | Tok::Rarrow
            | Tok::Ellipsis
            | Tok::ColonEqual
            | Tok::Colon
    )
}

pub fn is_skip_comment_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Newline | Tok::Indent | Tok::Dedent | Tok::NonLogicalNewline | Tok::Comment { .. }
    )
}

pub fn is_soft_keyword_token(token: &Tok) -> bool {
    matches!(token, Tok::Match | Tok::Case)
}

pub fn is_arithmetic_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::DoubleStar | Tok::Star | Tok::Plus | Tok::Minus | Tok::Slash | Tok::At
    )
}

pub fn is_ws_optional_token(token: &Tok) -> bool {
    is_arithmetic_token(token)
        || matches!(
            token,
            Tok::CircumFlex
                | Tok::Amper
                | Tok::Vbar
                | Tok::LeftShift
                | Tok::RightShift
                | Tok::Percent
        )
}

pub fn is_ws_needed_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::DoubleStarEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::DoubleSlashEqual
            | Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::NotEqual
            | Tok::Less
            | Tok::Greater
            | Tok::PercentEqual
            | Tok::CircumflexEqual
            | Tok::AmperEqual
            | Tok::VbarEqual
            | Tok::EqEqual
            | Tok::LessEqual
            | Tok::GreaterEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::Equal
            | Tok::And
            | Tok::Or
            | Tok::In
            | Tok::Is
            | Tok::Rarrow
    )
}

pub fn is_unary_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Plus | Tok::Minus | Tok::Star | Tok::DoubleStar | Tok::RightShift
    )
}
