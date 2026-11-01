use ruff_python_ast::{CmpOp, Expr, ExprCompare};

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
pub(crate) fn compare_to_empty_string(checker: &Checker, compare: ExprCompare<'_>) {
    if !compare.operands().iter().any(is_empty_string) {
        return;
    }

    // Omit string comparisons within subscripts used for DataFrame and ndarray indexing.
    if checker
        .semantic()
        .current_expressions()
        .any(Expr::is_subscript_expr)
    {
        return;
    }

    let mut first = true;
    for (lhs, op, rhs) in compare.iter() {
        let Some(op) = EmptyStringCmpOp::from_operator(op) else {
            continue;
        };
        if std::mem::take(&mut first) && is_empty_string(lhs) {
            let literal = checker.generator().expr(lhs);
            let expr = checker.generator().expr(rhs);
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
        if is_empty_string(rhs) {
            let expr = checker.generator().expr(lhs);
            let literal = checker.generator().expr(rhs);
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

fn is_empty_string(expr: Expr<'_>) -> bool {
    expr.as_string_literal_expr()
        .is_some_and(|literal| literal.value().is_empty())
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EmptyStringCmpOp {
    Is,
    IsNot,
    Eq,
    NotEq,
}

impl EmptyStringCmpOp {
    fn from_operator(op: CmpOp) -> Option<Self> {
        match op {
            CmpOp::Is => Some(Self::Is),
            CmpOp::IsNot => Some(Self::IsNot),
            CmpOp::Eq => Some(Self::Eq),
            CmpOp::NotEq => Some(Self::NotEq),
            _ => None,
        }
    }

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
