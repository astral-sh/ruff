use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Operator};
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::rules::flake8_implicit_str_concat::settings::Settings;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for implicitly concatenated strings on a single line.
    ///
    /// ## Why is this bad?
    /// While it is valid Python syntax to concatenate multiple string or byte
    /// literals implicitly (via whitespace delimiters), it is unnecessary and
    /// negatively affects code readability.
    ///
    /// In some cases, the implicit concatenation may also be unintentional, as
    /// autoformatters are capable of introducing single-line implicit
    /// concatenations when collapsing long lines.
    ///
    /// ## Example
    /// ```python
    /// z = "The quick " "brown fox."
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// z = "The quick brown fox."
    /// ```
    pub struct SingleLineImplicitStringConcatenation;
);
impl Violation for SingleLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals on one line")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for implicitly concatenated strings that span multiple lines.
    ///
    /// ## Why is this bad?
    /// For string literals that wrap across multiple lines, PEP 8 recommends
    /// the use of implicit string concatenation within parentheses instead of
    /// using a backslash for line continuation, as the former is more readable
    /// than the latter.
    ///
    /// By default, this rule will only trigger if the string literal is
    /// concatenated via a backslash. To disallow implicit string concatenation
    /// altogether, set the `flake8-implicit-str-concat.allow-multiline` option
    /// to `false`.
    ///
    /// ## Options
    /// * `flake8-implicit-str-concat.allow-multiline`
    ///
    /// ## Example
    /// ```python
    /// z = "The quick brown fox jumps over the lazy "\
    ///     "dog."
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// z = (
    ///     "The quick brown fox jumps over the lazy "
    ///     "dog."
    /// )
    /// ```
    ///
    /// ## References
    /// * [PEP 8](https://peps.python.org/pep-0008/#maximum-line-length)
    pub struct MultiLineImplicitStringConcatenation;
);
impl Violation for MultiLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals over multiple lines")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for string literals that are explicitly concatenated (using the
    /// `+` operator).
    ///
    /// ## Why is this bad?
    /// For string literals that wrap across multiple lines, implicit string
    /// concatenation within parentheses is preferred over explicit
    /// concatenation using the `+` operator, as the former is more readable.
    ///
    /// ## Example
    /// ```python
    /// z = (
    ///     "The quick brown fox jumps over the lazy "
    ///     + "dog"
    /// )
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// z = (
    ///     "The quick brown fox jumps over the lazy "
    ///     "dog"
    /// )
    /// ```
    pub struct ExplicitStringConcatenation;
);
impl Violation for ExplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Explicitly concatenated string should be implicitly concatenated")
    }
}

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
                    SingleLineImplicitStringConcatenation,
                    Range {
                        location: *a_start,
                        end_location: *b_end,
                    },
                ));
            } else {
                diagnostics.push(Diagnostic::new(
                    MultiLineImplicitStringConcatenation,
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
                    ExplicitStringConcatenation,
                    Range::from_located(expr),
                ));
            }
        }
    }
    None
}
