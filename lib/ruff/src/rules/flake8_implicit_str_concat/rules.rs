use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind, Operator};
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::rules::flake8_implicit_str_concat::settings::Settings;
use crate::violations;

/// ISC001, ISC002
pub fn implicit(tokens: &[LexResult], settings: &Settings) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    for ((a_start, a_tok, a_end), (b_start, b_tok, b_end)) in tokens
        .iter()
        .flatten()
        .filter(|(_, tok, _)| {
            !matches!(tok, Tok::Comment(..))
                && (settings.allow_multiline || !matches!(tok, Tok::NonLogicalNewline))
        })
        .tuple_windows()
    {
        if matches!(a_tok, Tok::String { .. }) && matches!(b_tok, Tok::String { .. }) {
            if a_end.row() == b_start.row() {
                diagnostics.push(Diagnostic::new(
                    violations::SingleLineImplicitStringConcatenation,
                    Range {
                        location: *a_start,
                        end_location: *b_end,
                    },
                ));
            } else {
                diagnostics.push(Diagnostic::new(
                    violations::MultiLineImplicitStringConcatenation,
                    Range {
                        location: *a_start,
                        end_location: *b_end,
                    },
                ));
            }
        }
    }
    diagnostics
}

/// ISC003
pub fn explicit(expr: &Expr) -> Option<Diagnostic> {
    if let ExprKind::BinOp { left, op, right } = &expr.node {
        if matches!(op, Operator::Add) {
            if matches!(
                left.node,
                ExprKind::JoinedStr { .. }
                    | ExprKind::Constant {
                        value: Constant::Str(..) | Constant::Bytes(..),
                        ..
                    }
            ) && matches!(
                right.node,
                ExprKind::JoinedStr { .. }
                    | ExprKind::Constant {
                        value: Constant::Str(..) | Constant::Bytes(..),
                        ..
                    }
            ) {
                return Some(Diagnostic::new(
                    violations::ExplicitStringConcatenation,
                    Range::from_located(expr),
                ));
            }
        }
    }
    None
}
