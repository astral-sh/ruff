use ruff_python_ast::flat::fields as ast;
use ruff_python_ast::{Expr, Number, flat};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;
use std::fmt;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "v0.0.278", category = Category::Correctness)]
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
            format!(
                "Slice in indexed access to type `{value_type}` uses type `{index_type}` instead of an integer"
            )
        } else {
            format!(
                "Indexed access to type `{value_type}` uses type `{index_type}` instead of an integer or slice"
            )
        }
    }
}

/// RUF016
pub(crate) fn invalid_index_type(checker: &Checker, expr: &ast::ExprSubscript<'_>) {
    let value = expr.value;
    let index = expr.slice;

    // Check the value being indexed is a list, tuple, string, f-string, bytes, or comprehension
    if !matches!(
        value.classify(),
        flat::ExprKind::List(_)
            | flat::ExprKind::ListComp(_)
            | flat::ExprKind::Tuple(_)
            | flat::ExprKind::FString(_)
            | flat::ExprKind::StringLiteral(_)
            | flat::ExprKind::BytesLiteral(_)
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

    if let flat::ExprKind::Slice(slice_node) = index.classify() {
        let lower = slice_node.lower();
        let upper = slice_node.upper();
        let step = slice_node.step();
        for is_slice in [lower, upper, step].into_iter().flatten() {
            let Some(is_slice_type) = CheckableExprType::try_from(is_slice) else {
                continue;
            };
            // A slice bound must be an integer, `None`, or a boolean.
            if !matches!(
                is_slice_type,
                CheckableExprType::IntLiteral
                    | CheckableExprType::NoneLiteral
                    | CheckableExprType::BooleanLiteral
            ) {
                checker.report_diagnostic(
                    InvalidIndexType {
                        value_type: value_type.to_string(),
                        index_type: is_slice_type.to_string(),
                        is_slice: true,
                    },
                    is_slice.range(),
                );
            }
        }
    } else if !matches!(
        index_type,
        CheckableExprType::IntLiteral | CheckableExprType::BooleanLiteral
    ) {
        // A non-slice index must be an integer or a boolean.
        checker.report_diagnostic(
            InvalidIndexType {
                value_type: value_type.to_string(),
                index_type: index_type.to_string(),
                is_slice: false,
            },
            index.range(),
        );
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
    TString,
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
    Generator,
    Lambda,
}

impl fmt::Display for CheckableExprType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::FString => f.write_str("str"),
            Self::TString => f.write_str("Template"),
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
            Self::Generator => f.write_str("generator"),
            Self::Lambda => f.write_str("lambda"),
        }
    }
}

impl CheckableExprType {
    fn try_from(expr: flat::View<'_, Expr>) -> Option<Self> {
        match expr.classify() {
            flat::ExprKind::StringLiteral(_) => Some(Self::StringLiteral),
            flat::ExprKind::BytesLiteral(_) => Some(Self::BytesLiteral),
            flat::ExprKind::NumberLiteral(expr_number_literal) => {
                let value = expr_number_literal.value();
                match value {
                    Number::Int(_) => Some(Self::IntLiteral),
                    Number::Float(_) => Some(Self::FloatLiteral),
                    Number::Complex { .. } => Some(Self::ComplexLiteral),
                }
            }
            flat::ExprKind::BooleanLiteral(_) => Some(Self::BooleanLiteral),
            flat::ExprKind::NoneLiteral(_) => Some(Self::NoneLiteral),
            flat::ExprKind::EllipsisLiteral(_) => Some(Self::EllipsisLiteral),
            flat::ExprKind::TString(_) => Some(Self::TString),
            flat::ExprKind::FString(_) => Some(Self::FString),
            flat::ExprKind::List(_) => Some(Self::List),
            flat::ExprKind::ListComp(_) => Some(Self::ListComp),
            flat::ExprKind::SetComp(_) => Some(Self::SetComp),
            flat::ExprKind::DictComp(_) => Some(Self::DictComp),
            flat::ExprKind::Set(_) => Some(Self::Set),
            flat::ExprKind::Dict(_) => Some(Self::Dict),
            flat::ExprKind::Tuple(_) => Some(Self::Tuple),
            flat::ExprKind::Slice(_) => Some(Self::Slice),
            flat::ExprKind::Generator(_) => Some(Self::Generator),
            flat::ExprKind::Lambda(_) => Some(Self::Lambda),
            _ => None,
        }
    }
}
