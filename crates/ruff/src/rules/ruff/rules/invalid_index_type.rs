use ruff_python_ast::{Constant, Expr, ExprConstant, ExprSlice, ExprSubscript};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct InvalidIndexType {
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
            format!("Slice in indexed access to type `{value_type}` uses type `{index_type}` instead of an integer.")
        } else {
            format!(
                "Indexed access to type `{value_type}` uses type `{index_type}` instead of an integer or slice."
            )
        }
    }
}

/// RUF016
pub(crate) fn invalid_index_type(checker: &mut Checker, expr: &ExprSubscript) {
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
            | Expr::Constant(ExprConstant {
                value: Constant::Str(_) | Constant::Bytes(_),
                ..
            })
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

    // Then check the contents of the index
    match index.as_ref() {
        Expr::Constant(ExprConstant {
            value: index_value, ..
        }) => {
            // If the index is a constant, require an integer
            if !index_value.is_int() {
                checker.diagnostics.push(Diagnostic::new(
                    InvalidIndexType {
                        value_type: value_type.to_string(),
                        index_type: constant_type_name(index_value).to_string(),
                        is_slice: false,
                    },
                    index.range(),
                ));
            }
        }
        Expr::Slice(ExprSlice {
            lower, upper, step, ..
        }) => {
            // If the index is a slice, require integer or null bounds
            for is_slice in [lower, upper, step].into_iter().flatten() {
                if let Expr::Constant(ExprConstant {
                    value: index_value, ..
                }) = is_slice.as_ref()
                {
                    if !(index_value.is_int() || index_value.is_none()) {
                        checker.diagnostics.push(Diagnostic::new(
                            InvalidIndexType {
                                value_type: value_type.to_string(),
                                index_type: constant_type_name(index_value).to_string(),
                                is_slice: true,
                            },
                            is_slice.range(),
                        ));
                    }
                } else if let Some(is_slice_type) = CheckableExprType::try_from(is_slice.as_ref()) {
                    checker.diagnostics.push(Diagnostic::new(
                        InvalidIndexType {
                            value_type: value_type.to_string(),
                            index_type: is_slice_type.to_string(),
                            is_slice: true,
                        },
                        is_slice.range(),
                    ));
                }
            }
        }
        _ => {
            // If it's some other checkable data type, it's a violation
            checker.diagnostics.push(Diagnostic::new(
                InvalidIndexType {
                    value_type: value_type.to_string(),
                    index_type: index_type.to_string(),
                    is_slice: false,
                },
                index.range(),
            ));
        }
    }
}

/// An expression that can be checked for type compatibility.
///
/// These are generally "literal" type expressions in that we know their concrete type
/// without additional analysis; opposed to expressions like a function call where we
/// cannot determine what type it may return.
#[derive(Debug)]
enum CheckableExprType<'a> {
    Constant(&'a Constant),
    FString,
    List,
    ListComp,
    SetComp,
    DictComp,
    Set,
    Dict,
    Tuple,
    Slice,
}

impl fmt::Display for CheckableExprType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Constant(constant) => f.write_str(constant_type_name(constant)),
            Self::FString => f.write_str("str"),
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

impl<'a> CheckableExprType<'a> {
    fn try_from(expr: &'a Expr) -> Option<Self> {
        match expr {
            Expr::Constant(ExprConstant { value, .. }) => Some(Self::Constant(value)),
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
}

fn constant_type_name(constant: &Constant) -> &'static str {
    match constant {
        Constant::None => "None",
        Constant::Bool(_) => "bool",
        Constant::Str(_) => "str",
        Constant::Bytes(_) => "bytes",
        Constant::Int(_) => "int",
        Constant::Float(_) => "float",
        Constant::Complex { .. } => "complex",
        Constant::Ellipsis => "ellipsis",
    }
}
