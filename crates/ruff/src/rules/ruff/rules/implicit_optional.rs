use std::fmt;

use anyhow::Result;
use rustpython_parser::ast::{self, Arguments, Constant, Expr, Operator, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::model::SemanticModel;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for the use of implicit `Optional` in type annotations when the
/// default parameter value is `None`.
///
/// ## Why is this bad?
/// Implicit `Optional` is prohibited by [PEP 484]. It is confusing and
/// inconsistent with the rest of the type system.
///
/// It's recommended to use `Optional[T]` instead. For Python 3.10 and later,
/// you can also use `T | None`.
///
/// ## Example
/// ```python
/// def foo(arg: int = None):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// from typing import Optional
///
///
/// def foo(arg: Optional[int] = None):
///     pass
/// ```
///
/// For Python 3.10 and later:
/// ```python
/// def foo(arg: int | None = None):
///     pass
/// ```
///
/// If you want to use the `|` operator in Python 3.9 and earlier, you can
/// use future imports:
/// ```python
/// from __future__ import annotations
///
///
/// def foo(arg: int | None = None):
///    pass
/// ```
///
/// [PEP 484]: https://peps.python.org/pep-0484/#union-types
#[violation]
pub struct ImplicitOptional {
    conversion_type: ConversionType,
}

impl AlwaysAutofixableViolation for ImplicitOptional {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("PEP 484 prohibits implicit `Optional`")
    }

    fn autofix_title(&self) -> String {
        format!("Convert to `{}`", self.conversion_type)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ConversionType {
    BinOpOr,
    Optional,
}

impl fmt::Display for ConversionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::BinOpOr => f.write_str("T | None"),
            Self::Optional => f.write_str("Optional[T]"),
        }
    }
}

impl ConversionType {
    fn from_target_version(target_version: PythonVersion) -> Self {
        if target_version >= PythonVersion::Py310 {
            Self::BinOpOr
        } else {
            Self::Optional
        }
    }
}

/// Custom iterator to collect all the `|`-separated expressions in a union.
struct PEP604UnionIterator<'a> {
    stack: Vec<&'a Expr>,
}

impl<'a> PEP604UnionIterator<'a> {
    fn new(expr: &'a Expr) -> Self {
        Self { stack: vec![expr] }
    }
}

impl<'a> Iterator for PEP604UnionIterator<'a> {
    type Item = &'a Expr;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(expr) = self.stack.pop() {
            match expr {
                Expr::BinOp(ast::ExprBinOp {
                    left,
                    op: Operator::BitOr,
                    right,
                    ..
                }) => {
                    self.stack.push(left);
                    self.stack.push(right);
                }
                _ => return Some(expr),
            }
        }
        None
    }
}

#[derive(Debug)]
enum TypingTarget<'a> {
    None,
    Optional,
    Union(Vec<&'a Expr>),
    Literal(Vec<&'a Expr>),
    Annotated(&'a Expr),
}

impl<'a> TypingTarget<'a> {
    fn try_from_expr(model: &SemanticModel, expr: &'a Expr) -> Option<Self> {
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if model.match_typing_expr(value, "Optional") {
                    return Some(TypingTarget::Optional);
                }
                let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() else{
                    return None;
                };
                if model.match_typing_expr(value, "Literal") {
                    Some(TypingTarget::Literal(elts.iter().collect()))
                } else if model.match_typing_expr(value, "Union") {
                    Some(TypingTarget::Union(elts.iter().collect()))
                } else if model.match_typing_expr(value, "Annotated") {
                    elts.first().map(TypingTarget::Annotated)
                } else {
                    None
                }
            }
            Expr::BinOp(..) => Some(TypingTarget::Union(
                PEP604UnionIterator::new(expr).collect(),
            )),
            Expr::Constant(ast::ExprConstant {
                value: Constant::None,
                ..
            }) => Some(TypingTarget::None),
            _ => None,
        }
    }
}

/// Check if the given [`TypingTarget`] explicitly allows `None`.
fn is_typing_target_with_none(model: &SemanticModel, target: &TypingTarget) -> bool {
    match target {
        TypingTarget::None | TypingTarget::Optional => true,
        TypingTarget::Literal(elts) => elts.iter().any(|elt| {
            let Some(new_target) = TypingTarget::try_from_expr(model, elt) else {
                return false;
            };
            // Literal can only contain `None`, a literal value, other `Literal`
            // or an enum value.
            match new_target {
                TypingTarget::None => true,
                TypingTarget::Literal(_) => is_typing_target_with_none(model, &new_target),
                _ => false,
            }
        }),
        TypingTarget::Union(elts) => elts.iter().any(|elt| {
            let Some(new_target) = TypingTarget::try_from_expr(model, elt) else {
                return false;
            };
            match new_target {
                TypingTarget::None => true,
                _ => is_typing_target_with_none(model, &new_target),
            }
        }),
        TypingTarget::Annotated(elt) => {
            let Some(new_target) = TypingTarget::try_from_expr(model, elt) else {
                return false;
            };
            match new_target {
                TypingTarget::None => true,
                _ => is_typing_target_with_none(model, &new_target),
            }
        }
    }
}

/// Check if the given annotation [`Expr`] explicitly allows `None`.
///
/// This function will return `None` if the annotation explicitly allows `None`
/// otherwise it will return the annotation itself. If it's a `Annotated` type,
/// then the inner type will be checked.
///
/// This function assumes that the annotation is a valid typing annotation expression.
fn type_hint_explicitly_allows_none<'a>(
    model: &SemanticModel,
    annotation: &'a Expr,
) -> Option<&'a Expr> {
    let Some(target) = TypingTarget::try_from_expr(model, annotation) else {
        return Some(annotation);
    };
    match target {
        // Short circuit on top level `None` or `Optional`
        TypingTarget::None | TypingTarget::Optional => None,
        // Top level `Annotated` node should check for the inner type and
        // return the inner type if it doesn't allow `None`. If `Annotated`
        // is found nested inside another type, then the outer type should
        // be returned.
        TypingTarget::Annotated(expr) => type_hint_explicitly_allows_none(model, expr),
        _ => {
            if is_typing_target_with_none(model, &target) {
                None
            } else {
                Some(annotation)
            }
        }
    }
}

/// Generate a [`Fix`] for the given [`Expr`] as per the current target version.
///
/// If the target version is 3.10 or higher, the pipe operator is used to
/// indicate that `None` is allowed. Otherwise, the `typing.Optional` type
/// is used.
fn generate_fix(checker: &Checker, expr: &Expr) -> Result<Fix> {
    if checker.settings.target_version >= PythonVersion::Py310 {
        let new_expr = Expr::BinOp(ast::ExprBinOp {
            left: Box::new(expr.clone()),
            op: Operator::BitOr,
            right: Box::new(Expr::Constant(ast::ExprConstant {
                value: Constant::None,
                kind: None,
                range: TextRange::default(),
            })),
            range: TextRange::default(),
        });
        let content = checker.generator().expr(&new_expr);
        #[allow(deprecated)]
        Ok(Fix::unspecified(Edit::range_replacement(
            content,
            expr.range(),
        )))
    } else {
        let (import_edit, binding) = checker.importer.get_or_import_symbol(
            &ImportRequest::import_from("typing", "Optional"),
            expr.start(),
            checker.semantic_model(),
        )?;
        let new_expr = Expr::Subscript(ast::ExprSubscript {
            range: TextRange::default(),
            value: Box::new(Expr::Name(ast::ExprName {
                id: binding.into(),
                ctx: ast::ExprContext::Store,
                range: TextRange::default(),
            })),
            slice: Box::new(expr.clone()),
            ctx: ast::ExprContext::Load,
        });
        let content = checker.generator().expr(&new_expr);
        #[allow(deprecated)]
        Ok(Fix::unspecified_edits(
            Edit::range_replacement(content, expr.range()),
            [import_edit],
        ))
    }
}

/// RUF011
pub(crate) fn implicit_optional(checker: &mut Checker, arguments: &Arguments) {
    for (arg, default) in arguments
        .kwonlyargs
        .iter()
        .rev()
        .zip(arguments.kw_defaults.iter().rev())
        .chain(
            arguments
                .args
                .iter()
                .rev()
                .chain(arguments.posonlyargs.iter().rev())
                .zip(arguments.defaults.iter().rev()),
        )
    {
        if !matches!(
            default,
            Expr::Constant(ast::ExprConstant {
                value: Constant::None,
                ..
            }),
        ) {
            continue;
        }
        let Some(annotation) = &arg.annotation else {
            continue
        };
        let Some(expr) = type_hint_explicitly_allows_none(checker.semantic_model(), annotation) else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(
            ImplicitOptional {
                conversion_type: ConversionType::from_target_version(
                    checker.settings.target_version,
                ),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| generate_fix(checker, expr));
        }
        checker.diagnostics.push(diagnostic);
    }
}
