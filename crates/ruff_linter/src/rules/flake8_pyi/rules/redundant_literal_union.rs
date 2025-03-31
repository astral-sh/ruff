use std::fmt;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    self as ast, Expr, ExprBinOp, ExprContext, ExprName, ExprSubscript, LiteralExpressionRef,
    Operator, PythonVersion,
};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::importer::ImportRequest;

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
#[derive(ViolationMetadata)]
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
pub(crate) fn redundant_literal_union<'a>(checker: &Checker, union: &'a Expr) {
    let mut typing_literal_exprs = Vec::new();
    let mut builtin_types_in_union = FxHashSet::default();
    let mut literal_subscript = None;
    let mut literal_exprs = Vec::new();
    let union_type =
        if (checker.target_version() >= PythonVersion::PY310) || checker.source_type.is_stub() {
            UnionKind::PEP604
        } else {
            UnionKind::TypingUnion
        };

    // Adds a member to `literal_exprs` for each value in a `Literal`, and any builtin types
    // to `builtin_types_in_union`.
    let mut func = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                literal_exprs.push(expr);

                if literal_subscript.is_none() {
                    literal_subscript = Some(value.as_ref());
                }

                if let Expr::Tuple(tuple) = &**slice {
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

    let mut diagnostics = Vec::new();
    let mut non_redundant_literal_types = Vec::new();
    let mut redundant_literal_types = Vec::new();

    for typing_literal_expr in typing_literal_exprs {
        let Some(literal_type) = match_literal_type(typing_literal_expr) else {
            continue;
        };

        if builtin_types_in_union.contains(&literal_type) {
            redundant_literal_types.push(typing_literal_expr);
            diagnostics.push(Diagnostic::new(
                RedundantLiteralUnion {
                    literal: SourceCodeSnippet::from_str(
                        checker.locator().slice(typing_literal_expr),
                    ),
                    builtin_type: literal_type,
                },
                typing_literal_expr.range(),
            ));
        } else {
            non_redundant_literal_types.push(typing_literal_expr);
        }
    }

    let mut new_literal_expr_types: Vec<LiteralExprType<'a>> = Vec::new();

    // Group all the non-redundant literal types together based on the `Literals`
    let mut func = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                let mut group = Vec::new();

                if let Expr::Tuple(tuple) = &**slice {
                    for tuple_slice in tuple {
                        if non_redundant_literal_types.contains(&tuple_slice) {
                            group.push(tuple_slice);
                        }
                    }
                } else {
                    if non_redundant_literal_types.contains(&slice.as_ref()) {
                        group.push(slice);
                    }
                }
                if !group.is_empty() {
                    new_literal_expr_types.push(LiteralExprType::NonRedundantTypes(group.clone()));
                }
            }
            return;
        }
        let Some(_) = match_builtin_type(expr, checker.semantic()) else {
            return;
        };
        new_literal_expr_types.push(LiteralExprType::BuiltinType(expr));
    };

    traverse_union(&mut func, checker.semantic(), union);

    let Some(literal_subscript) = literal_subscript else {
        return;
    };

    // This generates new individual `Literal` exprs and builtins
    let mut new_exprs = Vec::new();
    for group in new_literal_expr_types {
        match group {
            LiteralExprType::BuiltinType(expr) => {
                new_exprs.push(expr.to_owned());
            }
            LiteralExprType::NonRedundantTypes(group) => {
                let new_literal_expr = Expr::Subscript(ast::ExprSubscript {
                    value: Box::new(literal_subscript.clone()),
                    range: TextRange::default(),
                    ctx: ExprContext::Load,
                    slice: Box::new(if group.len() > 1 {
                        Expr::Tuple(ast::ExprTuple {
                            elts: group.into_iter().cloned().collect(),
                            range: TextRange::default(),
                            ctx: ExprContext::Load,
                            parenthesized: true,
                        })
                    } else {
                        group[0].clone()
                    }),
                });
                new_exprs.push(new_literal_expr);
            }
        }
    }

    // Now concatenate the exprs based on the union type
    let fix = match union_type {
        UnionKind::PEP604 => generate_pep604_fix(checker, new_exprs, union),
        UnionKind::TypingUnion => generate_typing_union_fix(checker, new_exprs, union),
    };

    for diagnostic in &mut diagnostics {
        diagnostic.set_fix(fix.clone());
    }

    checker.report_diagnostics(diagnostics);
}

fn generate_pep604_fix(checker: &Checker, new_exprs: Vec<Expr>, union: &Expr) -> Fix {
    let new_expr = new_exprs
        .into_iter()
        .fold(None, |acc: Option<Expr>, right: Expr| {
            if let Some(left) = acc {
                Some(Expr::BinOp(ExprBinOp {
                    left: Box::new(left),
                    op: Operator::BitOr,
                    right: Box::new(right),
                    range: TextRange::default(),
                }))
            } else {
                Some(right)
            }
        })
        .unwrap();

    Fix::safe_edit(Edit::range_replacement(
        checker.generator().expr(&new_expr),
        union.range(),
    ))
}

fn generate_typing_union_fix(checker: &Checker, new_exprs: Vec<Expr>, union: &Expr) -> Fix {
    let (import_edit, binding) = checker
        .importer()
        .get_or_import_symbol(
            &ImportRequest::import_from("typing", "Union"),
            union.range().start(),
            checker.semantic(),
        )
        .expect("Error importing");

    // Construct the expression as `Subscript[typing.Union, Tuple[expr, [expr, ...]]]`
    let new_expr = Expr::Subscript(ExprSubscript {
        range: TextRange::default(),
        value: Box::new(Expr::Name(ExprName {
            id: Name::new(binding),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })),
        slice: Box::new(if new_exprs.len() > 1 {
            Expr::Tuple(ast::ExprTuple {
                elts: new_exprs,
                range: TextRange::default(),
                ctx: ExprContext::Load,
                parenthesized: true,
            })
        } else {
            new_exprs[0].clone()
        }),
        ctx: ExprContext::Load,
    });

    Fix::applicable_edits(
        Edit::range_replacement(checker.generator().expr(&new_expr), union.range()),
        [import_edit],
        ruff_diagnostics::Applicability::Safe,
    )
}

#[derive(Debug, Clone)]
enum LiteralExprType<'a> {
    NonRedundantTypes(Vec<&'a Expr>),
    BuiltinType(&'a Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
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
