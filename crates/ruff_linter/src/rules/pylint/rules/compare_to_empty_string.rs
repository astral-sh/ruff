use anyhow::bail;
use ruff_python_ast::flat;
use ruff_python_ast::{CmpOp, ExprCompare};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for comparisons to empty strings.
///
/// ## Why is this bad?
/// An empty string is falsy, so it is unnecessary to compare it to `""`. If
/// the value can be something else Python considers falsy, such as `None`,
/// `0`, or another empty container, then the code is not equivalent.
///
/// ## Known problems
/// High false positive rate, as the check is context-insensitive and does not
/// consider the type of the variable being compared ([#4282]).
///
/// ## Example
/// ```python
/// x: str = ...
///
/// if x == "":
///     print("x is empty")
/// ```
///
/// Use instead:
/// ```python
/// x: str = ...
///
/// if not x:
///     print("x is empty")
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
///
/// [#4282]: https://github.com/astral-sh/ruff/issues/4282
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.255", category = Category::Pedantic)]
pub(crate) struct CompareToEmptyString {
    existing: String,
    replacement: String,
}

impl Violation for CompareToEmptyString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareToEmptyString {
            existing,
            replacement,
        } = self;
        format!("`{existing}` can be simplified to `{replacement}` as an empty string is falsey")
    }
}

/// PLC1901
pub(crate) fn compare_to_empty_string(checker: &Checker, compare: flat::View<'_, ExprCompare>) {
    // Omit string comparison rules within subscripts. This is mostly commonly used within
    // DataFrame and np.ndarray indexing.
    if checker
        .semantic()
        .current_expressions()
        .any(flat::View::is_subscript_expr)
    {
        return;
    }

    let mut first = true;
    for (lhs, op, rhs) in compare.iter() {
        if let Ok(op) = EmptyStringCmpOp::try_from(&op) {
            if std::mem::take(&mut first) {
                // Check the left-most expression.
                if let flat::ExprKind::StringLiteral(string_literal) = lhs.classify() {
                    let value = string_literal.value();
                    if value.is_empty() {
                        let literal = checker.generator().expr_view(lhs);
                        let expr = checker.generator().expr_view(rhs);
                        let existing = format!("{literal} {op} {expr}");
                        let replacement = format!("{}{expr}", op.into_unary());
                        checker.report_diagnostic(
                            CompareToEmptyString {
                                existing,
                                replacement,
                            },
                            lhs.range(),
                        );
                    }
                }
            }

            // Check all right-hand expressions.
            if let flat::ExprKind::StringLiteral(string_literal) = rhs.classify() {
                let value = string_literal.value();
                if value.is_empty() {
                    let expr = checker.generator().expr_view(lhs);
                    let literal = checker.generator().expr_view(rhs);
                    let existing = format!("{expr} {op} {literal}");
                    let replacement = format!("{}{expr}", op.into_unary());
                    checker.report_diagnostic(
                        CompareToEmptyString {
                            existing,
                            replacement,
                        },
                        rhs.range(),
                    );
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EmptyStringCmpOp {
    Is,
    IsNot,
    Eq,
    NotEq,
}

impl TryFrom<&CmpOp> for EmptyStringCmpOp {
    type Error = anyhow::Error;

    fn try_from(value: &CmpOp) -> Result<Self, Self::Error> {
        match value {
            CmpOp::Is => Ok(Self::Is),
            CmpOp::IsNot => Ok(Self::IsNot),
            CmpOp::Eq => Ok(Self::Eq),
            CmpOp::NotEq => Ok(Self::NotEq),
            _ => bail!("{value:?} cannot be converted to EmptyStringCmpOp"),
        }
    }
}

impl EmptyStringCmpOp {
    fn into_unary(self) -> &'static str {
        match self {
            Self::Is | Self::Eq => "not ",
            Self::IsNot | Self::NotEq => "",
        }
    }
}

impl std::fmt::Display for EmptyStringCmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let repr = match self {
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::Eq => "==",
            Self::NotEq => "!=",
        };
        write!(f, "{repr}")
    }
}
