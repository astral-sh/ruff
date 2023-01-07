use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind, Location, Operator};
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::source_code_locator::SourceCodeLocator;
use crate::violations;

/// ISC001, ISC002
pub fn implicit(tokens: &[LexResult], locator: &SourceCodeLocator) -> Vec<Diagnostic> {
    let mut checks = vec![];
    for ((a_start, a_tok, a_end), (b_start, b_tok, b_end)) in
        tokens.iter().flatten().tuple_windows()
    {
        if matches!(a_tok, Tok::String { .. }) && matches!(b_tok, Tok::String { .. }) {
            if a_end.row() == b_start.row() {
                checks.push(Diagnostic::new(
                    violations::SingleLineImplicitStringConcatenation,
                    Range {
                        location: *a_start,
                        end_location: *b_end,
                    },
                ));
            } else {
                // TODO(charlie): The RustPython tokenization doesn't differentiate between
                // continuations and newlines, so we have to detect them manually.
                let contents = locator.slice_source_code_range(&Range {
                    location: *a_end,
                    end_location: Location::new(a_end.row() + 1, 0),
                });
                if contents.trim_end().ends_with('\\') {
                    checks.push(Diagnostic::new(
                        violations::MultiLineImplicitStringConcatenation,
                        Range {
                            location: *a_start,
                            end_location: *b_end,
                        },
                    ));
                }
            }
        }
    }
    checks
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
