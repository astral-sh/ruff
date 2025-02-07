use ruff_python_ast::{Expr, ExprNumberLiteral, ExprSlice, ExprSubscript, Number};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;
use std::fmt;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for indexed access to lists, strings, tuples, bytes, and comprehensions
/// using a type other than an integer or slice.
///
/// ## Why is this bad?
/// Only integers or slices can be used as indices to these types. Using
/// other types will result in a `TypeError` at runtime and a `SyntaxWarning` at
/// import time.
///
/// ## Example
/// ```python
/// var = [1, 2, 3]["x"]
/// ```
///
/// Use instead:
/// ```python
/// var = [1, 2, 3][0]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidIndexType {
    value_type: String,
    index_type: String,
    is_slice: bool,
}

impl Violation for InvalidIndexType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidIndexType {
            value_type,
            index_type,
            is_slice,
        } = self;
        if *is_slice {
            format!("Slice in indexed access to type `{value_type}` uses type `{index_type}` instead of an integer")
        } else {
            format!(
                "Indexed access to type `{value_type}` uses type `{index_type}` instead of an integer or slice"
            )
        }
    }
}

/// RUF016
pub(crate) fn invalid_index_type(checker: &Checker, expr: &ExprSubscript) {
    let ExprSubscript {
        value,
        slice: index,
        ..
    } = expr;

    // Check the value being indexed is a list, tuple, string, f-string, bytes, or comprehension
    if !matches!(
        value.as_ref(),
        Expr::List(_)
            | Expr::ListComp(_)
            | Expr::Tuple(_)
            | Expr::FString(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
    ) {
        return;
    }

    // The value types supported by this rule should always be checkable
    let Some(value_type) = CheckableExprType::try_from(value) else {
        debug_assert!(
            false,
            "Index value must be a checkable type to generate a violation message."
        );
        return;
    };

    // If the index is not a checkable type then we can't easily determine if there is a violation
    let Some(index_type) = CheckableExprType::try_from(index) else {
        return;
    };

    if index_type.is_literal() {
        // If the index is a literal, require an integer
        if index_type != CheckableExprType::IntLiteral {
            checker.report_diagnostic(Diagnostic::new(
                InvalidIndexType {
                    value_type: value_type.to_string(),
                    index_type: index_type.to_string(),
                    is_slice: false,
                },
                index.range(),
            ));
        }
    } else if let Expr::Slice(ExprSlice {
        lower, upper, step, ..
    }) = index.as_ref()
    {
        for is_slice in [lower, upper, step].into_iter().flatten() {
            let Some(is_slice_type) = CheckableExprType::try_from(is_slice) else {
                return;
            };
            if is_slice_type.is_literal() {
                // If the index is a slice, require integer or null bounds
                if !matches!(
                    is_slice_type,
                    CheckableExprType::IntLiteral | CheckableExprType::NoneLiteral
                ) {
                    checker.report_diagnostic(Diagnostic::new(
                        InvalidIndexType {
                            value_type: value_type.to_string(),
                            index_type: is_slice_type.to_string(),
                            is_slice: true,
                        },
                        is_slice.range(),
                    ));
                }
            } else if let Some(is_slice_type) = CheckableExprType::try_from(is_slice.as_ref()) {
                checker.report_diagnostic(Diagnostic::new(
                    InvalidIndexType {
                        value_type: value_type.to_string(),
                        index_type: is_slice_type.to_string(),
                        is_slice: true,
                    },
                    is_slice.range(),
                ));
            }
        }
    } else {
        // If it's some other checkable data type, it's a violation
        checker.report_diagnostic(Diagnostic::new(
            InvalidIndexType {
                value_type: value_type.to_string(),
                index_type: index_type.to_string(),
                is_slice: false,
            },
            index.range(),
        ));
    }
}

/// An expression that can be checked for type compatibility.
///
/// These are generally "literal" type expressions in that we know their concrete type
/// without additional analysis; opposed to expressions like a function call where we
/// cannot determine what type it may return.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CheckableExprType {
    FString,
    StringLiteral,
    BytesLiteral,
    IntLiteral,
    FloatLiteral,
    ComplexLiteral,
    BooleanLiteral,
    NoneLiteral,
    EllipsisLiteral,
    List,
    ListComp,
    SetComp,
    DictComp,
    Set,
    Dict,
    Tuple,
    Slice,
}

impl fmt::Display for CheckableExprType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::FString => f.write_str("str"),
            Self::StringLiteral => f.write_str("str"),
            Self::BytesLiteral => f.write_str("bytes"),
            Self::IntLiteral => f.write_str("int"),
            Self::FloatLiteral => f.write_str("float"),
            Self::ComplexLiteral => f.write_str("complex"),
            Self::BooleanLiteral => f.write_str("bool"),
            Self::NoneLiteral => f.write_str("None"),
            Self::EllipsisLiteral => f.write_str("ellipsis"),
            Self::List => f.write_str("list"),
            Self::SetComp => f.write_str("set comprehension"),
            Self::ListComp => f.write_str("list comprehension"),
            Self::DictComp => f.write_str("dict comprehension"),
            Self::Set => f.write_str("set"),
            Self::Slice => f.write_str("slice"),
            Self::Dict => f.write_str("dict"),
            Self::Tuple => f.write_str("tuple"),
        }
    }
}

impl CheckableExprType {
    fn try_from(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::StringLiteral(_) => Some(Self::StringLiteral),
            Expr::BytesLiteral(_) => Some(Self::BytesLiteral),
            Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Int(_) => Some(Self::IntLiteral),
                Number::Float(_) => Some(Self::FloatLiteral),
                Number::Complex { .. } => Some(Self::ComplexLiteral),
            },
            Expr::BooleanLiteral(_) => Some(Self::BooleanLiteral),
            Expr::NoneLiteral(_) => Some(Self::NoneLiteral),
            Expr::EllipsisLiteral(_) => Some(Self::EllipsisLiteral),
            Expr::FString(_) => Some(Self::FString),
            Expr::List(_) => Some(Self::List),
            Expr::ListComp(_) => Some(Self::ListComp),
            Expr::SetComp(_) => Some(Self::SetComp),
            Expr::DictComp(_) => Some(Self::DictComp),
            Expr::Set(_) => Some(Self::Set),
            Expr::Dict(_) => Some(Self::Dict),
            Expr::Tuple(_) => Some(Self::Tuple),
            Expr::Slice(_) => Some(Self::Slice),
            _ => None,
        }
    }

    fn is_literal(self) -> bool {
        matches!(
            self,
            Self::StringLiteral
                | Self::BytesLiteral
                | Self::IntLiteral
                | Self::FloatLiteral
                | Self::ComplexLiteral
                | Self::BooleanLiteral
                | Self::NoneLiteral
                | Self::EllipsisLiteral
        )
    }
}
