use anyhow::Result;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    self as ast,
    helpers::{pep_604_union, typing_optional},
    name::Name,
    Expr, ExprBinOp, ExprContext, ExprNoneLiteral, ExprSubscript, Operator, PythonVersion,
};
use ruff_python_semantic::analyze::typing::{traverse_literal, traverse_union};
use ruff_text_size::{Ranged, TextRange};

use smallvec::SmallVec;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for redundant `Literal[None]` annotations.
///
/// ## Why is this bad?
/// While `Literal[None]` is a valid type annotation, it is semantically equivalent to `None`.
/// Prefer `None` over `Literal[None]` for both consistency and readability.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// Literal[None]
/// Literal[1, 2, 3, "foo", 5, None]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// None
/// Literal[1, 2, 3, "foo", 5] | None
/// ```
///
/// ## Fix safety and availability
/// This rule's fix is marked as safe unless the literal contains comments.
///
/// There is currently no fix available when applying the fix would lead to
/// a `TypeError` from an expression of the form `None | None` or when we
/// are unable to import the symbol `typing.Optional` and the Python version
/// is 3.9 or below.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
#[derive(ViolationMetadata)]
pub(crate) struct RedundantNoneLiteral {
    union_kind: UnionKind,
}

impl Violation for RedundantNoneLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.union_kind {
            UnionKind::NoUnion => "Use `None` rather than `Literal[None]`".to_string(),
            UnionKind::TypingOptional => {
                "Use `Optional[Literal[...]]` rather than `Literal[None, ...]` ".to_string()
            }
            UnionKind::BitOr => {
                "Use `Literal[...] | None` rather than `Literal[None, ...]` ".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(match self.union_kind {
            UnionKind::NoUnion => "Replace with `None`".to_string(),
            UnionKind::TypingOptional => "Replace with `Optional[Literal[...]]`".to_string(),
            UnionKind::BitOr => "Replace with `Literal[...] | None`".to_string(),
        })
    }
}

/// PYI061
pub(crate) fn redundant_none_literal<'a>(checker: &Checker, literal_expr: &'a Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_typing() {
        return;
    }

    let Expr::Subscript(ast::ExprSubscript {
        value: literal_subscript,
        ..
    }) = literal_expr
    else {
        return;
    };

    let mut none_exprs: SmallVec<[&ExprNoneLiteral; 1]> = SmallVec::new();
    let mut literal_elements = vec![];

    let mut partition_literal_elements = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::NoneLiteral(none_expr) = expr {
            none_exprs.push(none_expr);
        } else {
            literal_elements.push(expr);
        }
    };

    traverse_literal(&mut partition_literal_elements, semantic, literal_expr);

    if none_exprs.is_empty() {
        return;
    }

    let union_kind = if literal_elements.is_empty() {
        UnionKind::NoUnion
    } else if (checker.target_version() >= PythonVersion::PY310) || checker.source_type.is_stub() {
        UnionKind::BitOr
    } else {
        UnionKind::TypingOptional
    };

    // N.B. Applying the fix can leave an unused import to be fixed by the `unused-import` rule.
    for none_expr in none_exprs {
        let mut diagnostic =
            Diagnostic::new(RedundantNoneLiteral { union_kind }, none_expr.range());
        diagnostic.try_set_optional_fix(|| {
            create_fix(
                checker,
                literal_expr,
                literal_subscript,
                literal_elements.clone(),
                union_kind,
            )
        });
        checker.report_diagnostic(diagnostic);
    }
}

/// If possible, return a [`Fix`] for a violation of this rule.
///
/// Avoid producing code that would raise an exception when
/// `Literal[None] | None` would be fixed to `None | None`.
/// Instead, do not provide a fix. We don't need to worry about unions
/// that use [`typing.Union`], as `Union[None, None]` is valid Python.
/// See <https://github.com/astral-sh/ruff/issues/14567>.
///
/// [`typing.Union`]: https://docs.python.org/3/library/typing.html#typing.Union
fn create_fix(
    checker: &Checker,
    literal_expr: &Expr,
    literal_subscript: &Expr,
    literal_elements: Vec<&Expr>,
    union_kind: UnionKind,
) -> Result<Option<Fix>> {
    let semantic = checker.semantic();

    let enclosing_pep604_union = semantic
        .current_expressions()
        .skip(1)
        .take_while(|expr| {
            matches!(
                expr,
                Expr::BinOp(ExprBinOp {
                    op: Operator::BitOr,
                    ..
                })
            )
        })
        .last();

    if let Some(enclosing_pep604_union) = enclosing_pep604_union {
        let mut is_fixable = true;

        traverse_union(
            &mut |expr, _| {
                if matches!(expr, Expr::NoneLiteral(_)) {
                    is_fixable = false;
                }
                if expr != literal_expr {
                    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr {
                        if semantic.match_typing_expr(value, "Literal")
                            && matches!(**slice, Expr::NoneLiteral(_))
                        {
                            is_fixable = false;
                        }
                    }
                }
            },
            semantic,
            enclosing_pep604_union,
        );

        if !is_fixable {
            return Ok(None);
        }
    }

    let applicability = if checker.comment_ranges().intersects(literal_expr.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    if matches!(union_kind, UnionKind::NoUnion) {
        return Ok(Some(Fix::applicable_edit(
            Edit::range_replacement("None".to_string(), literal_expr.range()),
            applicability,
        )));
    }

    let new_literal_expr = Expr::Subscript(ast::ExprSubscript {
        value: Box::new(literal_subscript.clone()),
        range: TextRange::default(),
        ctx: ExprContext::Load,
        slice: Box::new(if literal_elements.len() > 1 {
            Expr::Tuple(ast::ExprTuple {
                elts: literal_elements.into_iter().cloned().collect(),
                range: TextRange::default(),
                ctx: ExprContext::Load,
                parenthesized: true,
            })
        } else {
            literal_elements[0].clone()
        }),
    });

    let fix = match union_kind {
        UnionKind::TypingOptional => {
            let (import_edit, bound_name) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from("typing", "Optional"),
                literal_expr.start(),
                checker.semantic(),
            )?;
            let optional_expr = typing_optional(new_literal_expr, Name::from(bound_name));
            let content = checker.generator().expr(&optional_expr);
            let optional_edit = Edit::range_replacement(content, literal_expr.range());
            Fix::applicable_edits(import_edit, [optional_edit], applicability)
        }
        UnionKind::BitOr => {
            let none_expr = Expr::NoneLiteral(ExprNoneLiteral {
                range: TextRange::default(),
            });
            let union_expr = pep_604_union(&[new_literal_expr, none_expr]);
            let content = checker.generator().expr(&union_expr);
            let union_edit = Edit::range_replacement(content, literal_expr.range());
            Fix::applicable_edit(union_edit, applicability)
        }
        // We dealt with this case earlier to avoid allocating `lhs` and `rhs`
        UnionKind::NoUnion => {
            unreachable!()
        }
    };
    Ok(Some(fix))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum UnionKind {
    NoUnion,
    TypingOptional,
    BitOr,
}
