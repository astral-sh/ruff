use std::fmt;

use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprKind, LiteralExpression};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for redundant unions between a `Literal` and a builtin supertype of
/// that `Literal`.
///
/// ## Why is this bad?
/// Using a `Literal` type in a union with its builtin supertype is redundant,
/// as the supertype will be strictly more general than the `Literal` type.
/// For example, `Literal["A"] | str` is equivalent to `str`, and
/// `Literal[1] | int` is equivalent to `int`, as `str` and `int` are the
/// supertypes of `"A"` and `1` respectively.
///
/// ## Example
/// ```pyi
/// from typing import Literal
///
/// x: Literal["A", b"B"] | str
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import Literal
///
/// x: Literal[b"B"] | str
/// ```
///
/// ## Known issues
/// This rule is opinionated and may not be appropriate for projects that keep
/// literal members for editor suggestions, generated documentation, or another
/// non-type-checking purpose. In those cases, disabling this rule for the
/// affected annotations may be reasonable.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.283", category = Category::Pedantic)]
pub(crate) struct RedundantLiteralUnion {
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
pub(crate) fn redundant_literal_union<'a>(checker: &Checker, &union: &Expr<'a>) {
    let mut typing_literal_exprs = Vec::new();
    let mut builtin_types_in_union = FxHashSet::default();

    // Adds a member to `literal_exprs` for each value in a `Literal`, and any builtin types
    // to `builtin_types_in_union`.
    let mut func = |expr: Expr<'a>, _parent: Expr<'a>| {
        if let ExprKind::Subscript(subscript) = expr.classify() {
            let value = subscript.value();
            let slice = subscript.slice();
            if checker.semantic().match_typing_expr(value, "Literal") {
                if let ExprKind::Tuple(tuple) = slice.classify() {
                    typing_literal_exprs.extend(tuple);
                } else {
                    typing_literal_exprs.push(slice);
                }
            }
            return;
        }

        let Some(builtin_type) = match_builtin_type(expr, checker.semantic()) else {
            return;
        };
        builtin_types_in_union.insert(builtin_type);
    };

    traverse_union(&mut func, checker.semantic(), union);

    for typing_literal_expr in typing_literal_exprs {
        let Some(literal_type) = match_literal_type(typing_literal_expr) else {
            continue;
        };

        if builtin_types_in_union.contains(&literal_type) {
            checker.report_diagnostic(
                RedundantLiteralUnion {
                    literal: SourceCodeSnippet::from_str(
                        checker.locator().slice(typing_literal_expr),
                    ),
                    builtin_type: literal_type,
                },
                typing_literal_expr.range(),
            );
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
fn match_builtin_type(expr: Expr<'_>, semantic: &SemanticModel) -> Option<ExprType> {
    let result = match semantic.resolve_builtin_symbol(expr)? {
        "int" => ExprType::Int,
        "bool" => ExprType::Bool,
        "str" => ExprType::Str,
        "float" => ExprType::Float,
        "bytes" => ExprType::Bytes,
        "complex" => ExprType::Complex,
        _ => return None,
    };
    Some(result)
}

/// Return the [`ExprType`] of an [`Expr`] if it is a literal (e.g., an `int`, like `1`, or a
/// `bool`, like `True`).
fn match_literal_type(expr: Expr<'_>) -> Option<ExprType> {
    Some(match expr.as_literal_expr()? {
        LiteralExpression::BooleanLiteral(_) => ExprType::Bool,
        LiteralExpression::StringLiteral(_) => ExprType::Str,
        LiteralExpression::BytesLiteral(_) => ExprType::Bytes,
        LiteralExpression::NumberLiteral(number) => match number.value() {
            ast::Number::Int(_) => ExprType::Int,
            ast::Number::Float(_) => ExprType::Float,
            ast::Number::Complex { .. } => ExprType::Complex,
        },
        LiteralExpression::NoneLiteral(_) | LiteralExpression::EllipsisLiteral(_) => {
            return None;
        }
    })
}
