use std::fmt;

use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr, LiteralExpressionRef};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::remove_member;
use crate::fix::snippet::SourceCodeSnippet;
use crate::preview::is_redundant_literal_union_fix_enabled;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// The fix deletes the redundant member, or the whole `Literal` when every member of it is
/// redundant. It is marked unsafe when the deletion reaches a comment, which is then deleted with
/// it or left describing whatever follows.
///
/// The surrounding union is left alone, so `typing.Union[...]` stays `typing.Union[...]` and
/// `X | Y` stays `X | Y`. Deleting all but one member of a `typing.Union[...]` therefore leaves a
/// single-element `typing.Union[X]`.
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

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

    fn fix_title(&self) -> Option<String> {
        Some("Remove redundant literal member".to_string())
    }
}

/// PYI051
pub(crate) fn redundant_literal_union<'a>(checker: &Checker, union: &'a Expr) {
    let mut literal_members: Vec<LiteralMember<'a>> = Vec::new();
    let mut builtin_types_in_union = FxHashSet::default();

    // A builtin counts only as a union member in its own right, never as a `Literal` member: the
    // early return below keeps the contents of a `Literal` out of `builtin_types_in_union`.
    let mut func = |expr: &'a Expr, parent: &'a Expr| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                let elements = match &**slice {
                    // Ex) `Literal["A", b"B"]`
                    Expr::Tuple(tuple) => tuple.elts.as_slice(),
                    // Ex) `Literal["A"]`
                    element => std::slice::from_ref(element),
                };
                literal_members.push(LiteralMember {
                    subscript: expr,
                    parent,
                    elements,
                });
            }
            return;
        }

        let Some(builtin_type) = match_builtin_type(expr, checker.semantic()) else {
            return;
        };
        builtin_types_in_union.insert(builtin_type);
    };

    traverse_union(&mut func, checker.semantic(), union);

    // A "complex" stringized annotation (implicit concatenation, escapes) is reparsed from a
    // buffer whose offsets do not map back to the source, so its ranges cannot become edits.
    let fix_enabled = is_redundant_literal_union_fix_enabled(checker.settings())
        && !checker.semantic().in_complex_string_type_definition();

    for member in &literal_members {
        let redundant: Vec<(usize, ExprType, &Expr)> = member
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                let literal_type = match_literal_type(element)?;
                builtin_types_in_union.contains(&literal_type).then_some((
                    index,
                    literal_type,
                    element,
                ))
            })
            .collect();

        if redundant.is_empty() {
            continue;
        }

        // `Literal[]` is a syntax error, so a `Literal` whose every member is redundant goes
        // entirely. That is one deletion, shared by every diagnostic raised for this `Literal`.
        let remove_subscript = redundant.len() == member.elements.len();
        let subscript_fix = if remove_subscript && fix_enabled {
            remove_literal_subscript(checker, member)
        } else {
            None
        };

        for &(index, literal_type, element) in &redundant {
            let mut diagnostic = checker.report_diagnostic(
                RedundantLiteralUnion {
                    literal: SourceCodeSnippet::from_str(checker.locator().slice(element)),
                    builtin_type: literal_type,
                },
                element.range(),
            );

            if !fix_enabled {
                continue;
            }

            if remove_subscript {
                if let Some(fix) = &subscript_fix {
                    diagnostic.set_fix(fix.clone());
                }
            } else if let Ok(edit) =
                remove_member(member.elements, index, checker.locator().contents())
            {
                diagnostic.set_fix(deletion_fix(checker, edit));
            }
        }
    }
}

/// A `Literal[...]` member of a union.
struct LiteralMember<'a> {
    /// The `Literal[...]` subscript itself.
    subscript: &'a Expr,
    /// The `x | y` or `Union[x, y]` expression that directly contains `subscript`.
    parent: &'a Expr,
    /// The members of the `Literal`, e.g. `"A"` and `b"B"` in `Literal["A", b"B"]`.
    elements: &'a [Expr],
}

/// Build a [`Fix`] that deletes an entire `Literal[...]` from the union that contains it.
///
/// Returns `None` when no range deletion can express it: when it would leave an empty `Union[]`,
/// when the `Literal` is parenthesized inside a `Union[...]` and the deletion would strip one half
/// of the pair, or when the union is not a shape `traverse_union` reports.
fn remove_literal_subscript(checker: &Checker, member: &LiteralMember) -> Option<Fix> {
    let source = checker.locator().contents();
    let comment_ranges = checker.comment_ranges();

    let edit = match member.parent {
        // Ex) `str | Literal["A"]`. Delete the `Literal` and the `|` joining it to the other
        // operand, each operand taken with its own parentheses so no half pair is left behind.
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            let binop = member.parent.into();
            let left = parenthesized_range(left.into(), binop, comment_ranges, source)
                .unwrap_or(left.range());
            let right = parenthesized_range(right.into(), binop, comment_ranges, source)
                .unwrap_or(right.range());

            let subscript = member.subscript.range();
            if left.contains_range(subscript) {
                Edit::deletion(left.start(), right.start())
            } else if right.contains_range(subscript) {
                Edit::deletion(left.end(), right.end())
            } else {
                return None;
            }
        }
        // Ex) `Union[Literal["A"], str]`. Delete the `Literal` and one of its neighbouring commas.
        Expr::Subscript(ast::ExprSubscript { slice, .. }) => {
            // Anything else is `Union[Literal["A"]]`, whose only member cannot be deleted.
            let Expr::Tuple(tuple) = &**slice else {
                return None;
            };
            // Ex) `Union[Literal["A"],]`, likewise.
            if tuple.elts.len() < 2 {
                return None;
            }
            let index = tuple
                .elts
                .iter()
                .position(|element| element.range() == member.subscript.range())?;
            if parenthesized_range(
                member.subscript.into(),
                slice.as_ref().into(),
                comment_ranges,
                source,
            )
            .is_some()
            {
                return None;
            }
            remove_member(&tuple.elts, index, source).ok()?
        }
        // `traverse_union` only ever reports the two union forms above as a member's parent.
        _ => return None,
    };

    Some(deletion_fix(checker, edit))
}

/// Wrap a deletion in a [`Fix`], marking it unsafe if the deletion reaches a comment.
///
/// A comment inside the range goes with it; one that merely abuts the range survives but is left
/// describing whatever now follows it.
fn deletion_fix(checker: &Checker, edit: Edit) -> Fix {
    let applicability = if checker.comment_ranges().intersects(edit.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };
    Fix::applicable_edit(edit, applicability)
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
fn match_literal_type(expr: &Expr) -> Option<ExprType> {
    Some(match expr.as_literal_expr()? {
        LiteralExpressionRef::BooleanLiteral(_) => ExprType::Bool,
        LiteralExpressionRef::StringLiteral(_) => ExprType::Str,
        LiteralExpressionRef::BytesLiteral(_) => ExprType::Bytes,
        LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => match value {
            ast::Number::Int(_) => ExprType::Int,
            ast::Number::Float(_) => ExprType::Float,
            ast::Number::Complex { .. } => ExprType::Complex,
        },
        LiteralExpressionRef::NoneLiteral(_) | LiteralExpressionRef::EllipsisLiteral(_) => {
            return None;
        }
    })
}
