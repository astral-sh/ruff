use rustpython_parser::ast::{Constant, Expr, ExprConstant, ExprSlice, ExprSubscript, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for indexed access to lists, strings, tuples, bytes, and comprehensions
/// using a type other than an integer or slice.
///
/// ## Why is this bad?
/// Only integers or slices can be used as indices to these types. Using
/// other types will result in a `TypeError`.
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
    var_type: String,
    idx_type: String,
    slice_bound: bool,
}

impl Violation for InvalidIndexType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidIndexType {
            var_type, idx_type, ..
        } = self;
        if self.slice_bound {
            format!("Slice in indexed access to type `{var_type}` uses type `{idx_type}` as bound instead of an integer.")
        } else {
            format!(
                "Indexed access to type `{var_type}` uses type `{idx_type}` instead of an integer or slice."
            )
        }
    }
}

/// RUF015
pub(crate) fn invalid_index_type<'a>(checker: &mut Checker, expr: &'a ExprSubscript) {
    let ExprSubscript { value, slice, .. } = expr;

    // If the value being indexed is a list, tuple, string, f-string, bytes, or comprehension
    if matches!(
        value.as_ref(),
        Expr::List(_)
            | Expr::ListComp(_)
            | Expr::Tuple(_)
            | Expr::JoinedStr(_)
            | Expr::Constant(ExprConstant {
                value: Constant::Str(_) | Constant::Bytes(_),
                ..
            })
    ) {
        // Then check the contents of the index
        match slice.as_ref() {
            // If the index is a const, only allow integers
            Expr::Constant(ExprConstant {
                value: index_value, ..
            }) => {
                if !index_value.is_int() {
                    checker.diagnostics.push(Diagnostic::new(
                        InvalidIndexType {
                            var_type: expression_type_name(value)
                                .expect("Failed to cast parent expression to type name")
                                .to_string(),
                            idx_type: constant_type_name(index_value).to_string(),
                            slice_bound: false,
                        },
                        slice.range(),
                    ));
                }
            }
            // If the index is a slice, check for integer or null bounds
            Expr::Slice(ExprSlice { lower, upper, .. }) => {
                for slice_bound in [lower, upper].into_iter().flatten() {
                    if let Expr::Constant(ExprConstant {
                        value: index_value, ..
                    }) = slice_bound.as_ref()
                    {
                        if !(index_value.is_int() || index_value.is_none()) {
                            checker.diagnostics.push(Diagnostic::new(
                                InvalidIndexType {
                                    var_type: expression_type_name(value)
                                        .expect("Failed to cast parent expression to type name")
                                        .to_string(),
                                    idx_type: constant_type_name(index_value).to_string(),
                                    slice_bound: true,
                                },
                                slice_bound.range(),
                            ));
                        }
                    } else if matches!(
                        slice_bound.as_ref(),
                        Expr::Tuple(_)
                            | Expr::List(_)
                            | Expr::Set(_)
                            | Expr::Dict(_)
                            | Expr::ListComp(_)
                            | Expr::DictComp(_)
                            | Expr::JoinedStr(_)
                    ) {
                        checker.diagnostics.push(Diagnostic::new(
                            InvalidIndexType {
                                var_type: expression_type_name(value)
                                    .expect("Failed to cast parent expression to type name")
                                    .to_string(),
                                idx_type: expression_type_name(slice_bound)
                                    .expect("Failed to slice bound expression to type name")
                                    .to_string(),
                                slice_bound: true,
                            },
                            slice_bound.range(),
                        ));
                    }
                }
            }
            // If it's some other data type, it's a violation
            Expr::Tuple(_)
            | Expr::List(_)
            | Expr::Set(_)
            | Expr::Dict(_)
            | Expr::ListComp(_)
            | Expr::DictComp(_)
            | Expr::JoinedStr(_) => {
                checker.diagnostics.push(Diagnostic::new(
                    InvalidIndexType {
                        var_type: expression_type_name(value)
                            .expect("Failed to cast parent expression to type name")
                            .to_string(),
                        idx_type: expression_type_name(slice)
                            .expect("Failed to cast index expression to type name")
                            .to_string(),
                        slice_bound: false,
                    },
                    slice.range(),
                ));
            }
            // If it's anything else, it's too hard to tell if it's a violation
            _ => (),
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
        Constant::Tuple(_) => "tuple",
        Constant::Float(_) => "float",
        Constant::Complex { .. } => "complex",
        Constant::Ellipsis => "ellipsis",
    }
}

/// Utility for casting expressions to their Python type name
/// Does not cover all cases, only implements expressions needed in RUF015
fn expression_type_name(expr: &Expr) -> Option<&'static str> {
    match expr {
        Expr::Constant(ExprConstant { value, .. }) => Some(constant_type_name(value)),
        Expr::JoinedStr(_) => Some("str"),
        Expr::List(_) => Some("list"),
        Expr::ListComp(_) => Some("list comprehension"),
        Expr::DictComp(_) => Some("dict comprehension"),
        Expr::Set(_) => Some("set"),
        Expr::Dict(_) => Some("dict"),
        Expr::Tuple(_) => Some("tuple"),
        _ => None,
    }
}
