use ast::{Constant, Ranged};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::{checkers::ast::Checker, rules::flake8_pyi::helpers::traverse_union};

/// ## What it does
/// Checks for the presence of redundant `Literal` types and builtin super
/// types in an union.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// from typing import Literal, Union
///
/// Redundancy: Literal["foo"] | str
/// Redundancy2: Union[Literal[2], int]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal, TypeAlias
///
/// WithoutRedundancy: Literal["foo"]
/// WithoutRedundancy2: Literal[2]
/// ```
#[violation]
pub struct RedundantLiteralUnion {
    literal: String,
    builtin_type: ExprType,
}

impl Violation for RedundantLiteralUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantLiteralUnion {
            literal,
            builtin_type,
        } = self;
        format!(
            "`Literal[{literal}]` is redundant in an union with `{}`",
            builtin_type.as_str()
        )
    }
}

/// PYI051
pub(crate) fn redundant_literal_union<'a>(checker: &mut Checker, union: &'a Expr) {
    let mut literal_exprs = SmallVec::<[&Expr; 1]>::new();
    let mut builtin_types_in_union = FxHashSet::default();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation and
    // adds the type of `expr` to `builtin_types_in_union`.
    let mut func = |expr: &'a Expr, _| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                literal_exprs.push(slice);
            }
            return;
        }

        let Some(builtin_type) = get_expr_type(expr) else {
            return;
        };
        builtin_types_in_union.insert(builtin_type);
    };

    traverse_union(&mut func, checker.semantic(), union, None);

    for literal_expr in literal_exprs {
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = literal_expr {
            handle_literal_with_multiple_members(checker, literal_expr, elts);
            continue;
        }

        let Some(literal_type) = get_expr_type(literal_expr) else {
            continue;
        };

        if builtin_types_in_union.contains(&literal_type) {
            checker.diagnostics.push(Diagnostic::new(
                RedundantLiteralUnion {
                    literal: checker.locator().slice(literal_expr.range()).to_string(),
                    builtin_type: literal_type,
                },
                literal_expr.range(),
            ));
        }
    }
}

/// Handles redundancy for `Literal`s with multiple members.
///
/// For example:
/// ```python
/// from typing import TypeAlias, Union, Literal
///
/// A: TypeAlias = Union[Literal["foo", "bar"], str]
/// ```
/// This function allows to show in the diagnostic the full expression,
/// e.g. `Literal["foo", "bar"]`, if its members are all of the same type.
///
/// If the members of `Literal` have different types like in the following,
/// example:
/// ```python
/// from typing import TypeAlias, Union, Literal
///
/// A: TypeAlias = Union[Literal[b"foo", "bar"], str]
/// ```
/// Only the `Literal` with the redundancy will be shown, e.g. `Literal["bar"]`.
fn handle_literal_with_multiple_members(
    checker: &mut Checker,
    literal: &Expr,
    literal_members: &[Expr],
) {
    let literal_members_types: Vec<ExprType> =
        literal_members.iter().filter_map(get_expr_type).collect();

    // Check if all `ExprType`s in `literal_members_types` is the same,
    // if it's true, handle `Literal` with members of the same type
    let literal_type = literal_members_types[0];
    if literal_members_types
        .iter()
        .all(|&element_type| element_type == literal_type)
    {
        checker.diagnostics.push(Diagnostic::new(
            RedundantLiteralUnion {
                literal: checker.locator().slice(literal.range()).to_string(),
                builtin_type: literal_type,
            },
            literal.range(),
        ));

        return;
    }

    // Handle `Literal` with different members types
    for literal_member in literal_members {
        let Some(literal_type) = get_expr_type(literal_member) else {
            continue;
        };

        checker.diagnostics.push(Diagnostic::new(
            RedundantLiteralUnion {
                literal: checker.locator().slice(literal_member.range()).to_string(),
                builtin_type: literal_type,
            },
            literal_member.range(),
        ));
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

impl ExprType {
    fn as_str(&self) -> &str {
        match self {
            ExprType::Int => "int",
            ExprType::Str => "str",
            ExprType::Bool => "bool",
            ExprType::Float => "float",
            ExprType::Bytes => "bytes",
            ExprType::Complex => "complex",
        }
    }
}

/// Return type of `expr` if it is a literal type ( e.g. `1`, `b"bytes"`, `"str"`,
/// `True`, `3.14` and `1J`) or it is a builtin type (e.g. `int`, `bool`, `float`, `str`,
/// `bytes` and `complex`).
fn get_expr_type(expr: &Expr) -> Option<ExprType> {
    if let Expr::Constant(ast::ExprConstant { value, .. }) = expr {
        return Some(match value {
            Constant::Bool(_) => ExprType::Bool,
            Constant::Str(_) => ExprType::Str,
            Constant::Bytes(_) => ExprType::Bytes,
            Constant::Int(_) => ExprType::Int,
            Constant::Float(_) => ExprType::Float,
            Constant::Complex { .. } => ExprType::Complex,
            _ => return None,
        });
    }

    if let Expr::Name(ast::ExprName { id, .. }) = expr {
        return Some(match id.as_str() {
            "int" => ExprType::Int,
            "bool" => ExprType::Bool,
            "str" => ExprType::Str,
            "float" => ExprType::Float,
            "bytes" => ExprType::Bytes,
            "complex" => ExprType::Complex,
            _ => return None,
        });
    }

    None
}
