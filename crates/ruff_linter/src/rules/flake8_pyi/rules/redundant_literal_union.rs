use rustc_hash::FxHashSet;
use std::fmt;

use ast::Constant;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::autofix::snippet::SourceCodeSnippet;
use crate::{checkers::ast::Checker, rules::flake8_pyi::helpers::traverse_union};

/// ## What it does
/// Checks for the presence of redundant `Literal` types and builtin super
/// types in an union.
///
/// ## Why is this bad?
/// The use of `Literal` types in a union with the builtin super type of one of
/// its literal members is redundant, as the super type is strictly more
/// general than the `Literal` type.
///
/// For example, `Literal["A"] | str` is equivalent to `str`, and
/// `Literal[1] | int` is equivalent to `int`, as `str` and `int` are the super
/// types of `"A"` and `1` respectively.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// A: Literal["A"] | str
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// A: Literal["A"]
/// ```
#[violation]
pub struct RedundantLiteralUnion {
    literal: SourceCodeSnippet,
    builtin_type: ExprType,
}

impl Violation for RedundantLiteralUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantLiteralUnion {
            literal,
            builtin_type,
        } = self;
        if let Some(literal) = literal.full_display() {
            format!("`Literal[{literal}]` is redundant in a union with `{builtin_type}`")
        } else {
            format!("`Literal` is redundant in a union with `{builtin_type}`")
        }
    }
}

/// PYI051
pub(crate) fn redundant_literal_union<'a>(checker: &mut Checker, union: &'a Expr) {
    let mut literal_exprs = Vec::new();
    let mut builtin_types_in_union = FxHashSet::default();

    // Adds a member to `literal_exprs` for each value in a `Literal`, and any builtin types
    // to `builtin_types_in_union`.
    let mut func = |expr: &'a Expr, _| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                    literal_exprs.extend(elts.iter());
                } else {
                    literal_exprs.push(slice);
                }
            }
            return;
        }

        let Some(builtin_type) = match_builtin_type(expr, checker.semantic()) else {
            return;
        };
        builtin_types_in_union.insert(builtin_type);
    };

    traverse_union(&mut func, checker.semantic(), union, None);

    for literal_expr in literal_exprs {
        let Some(constant_type) = match_constant_type(literal_expr) else {
            continue;
        };

        if builtin_types_in_union.contains(&constant_type) {
            checker.diagnostics.push(Diagnostic::new(
                RedundantLiteralUnion {
                    literal: SourceCodeSnippet::from_str(checker.locator().slice(literal_expr)),
                    builtin_type: constant_type,
                },
                literal_expr.range(),
            ));
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
enum ExprType {
    Int,
    Str,
    Bool,
    Float,
    Bytes,
    Complex,
}

impl fmt::Display for ExprType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Int => fmt.write_str("int"),
            Self::Str => fmt.write_str("str"),
            Self::Bool => fmt.write_str("bool"),
            Self::Float => fmt.write_str("float"),
            Self::Bytes => fmt.write_str("bytes"),
            Self::Complex => fmt.write_str("complex"),
        }
    }
}

/// Return the [`ExprType`] of an [`Expr]` if it is a builtin type (e.g. `int`, `bool`, `float`,
/// `str`, `bytes`, or `complex`).
fn match_builtin_type(expr: &Expr, semantic: &SemanticModel) -> Option<ExprType> {
    let name = expr.as_name_expr()?;
    let result = match name.id.as_str() {
        "int" => ExprType::Int,
        "bool" => ExprType::Bool,
        "str" => ExprType::Str,
        "float" => ExprType::Float,
        "bytes" => ExprType::Bytes,
        "complex" => ExprType::Complex,
        _ => return None,
    };
    if !semantic.is_builtin(name.id.as_str()) {
        return None;
    }
    Some(result)
}

/// Return the [`ExprType`] of an [`Expr]` if it is a constant (e.g., an `int`, like `1`, or a
/// `bool`, like `True`).
fn match_constant_type(expr: &Expr) -> Option<ExprType> {
    let constant = expr.as_constant_expr()?;
    let result = match constant.value {
        Constant::Bool(_) => ExprType::Bool,
        Constant::Str(_) => ExprType::Str,
        Constant::Bytes(_) => ExprType::Bytes,
        Constant::Int(_) => ExprType::Int,
        Constant::Float(_) => ExprType::Float,
        Constant::Complex { .. } => ExprType::Complex,
        _ => return None,
    };
    Some(result)
}
