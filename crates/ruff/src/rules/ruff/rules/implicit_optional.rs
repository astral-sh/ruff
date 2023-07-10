use std::fmt;

use anyhow::Result;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, ArgWithDefault, Arguments, Constant, Expr, Operator, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::typing::parse_type_annotation;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::sys::is_known_standard_library;

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
/// Or, for Python 3.10 and later:
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
///     pass
/// ```
///
/// ## Limitations
///
/// Type aliases are not supported and could result in false negatives.
/// For example, the following code will not be flagged:
/// ```python
/// Text = str | bytes
///
///
/// def foo(arg: Text = None):
///     pass
/// ```
///
/// ## Options
/// - `target-version`
///
/// [PEP 484]: https://peps.python.org/pep-0484/#union-types
#[violation]
pub struct ImplicitOptional {
    conversion_type: ConversionType,
}

impl Violation for ImplicitOptional {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("PEP 484 prohibits implicit `Optional`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some(format!("Convert to `{}`", self.conversion_type))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversionType {
    /// Conversion using the `|` operator e.g., `str | None`
    BinOpOr,
    /// Conversion using the `typing.Optional` type e.g., `typing.Optional[str]`
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

impl From<PythonVersion> for ConversionType {
    fn from(target_version: PythonVersion) -> Self {
        if target_version >= PythonVersion::Py310 {
            Self::BinOpOr
        } else {
            Self::Optional
        }
    }
}

/// Custom iterator to collect all the `|` separated expressions in a PEP 604
/// union type.
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

/// Returns `true` if the given call path is a known type.
///
/// A known type is either a builtin type, any object from the standard library,
/// or a type from the `typing_extensions` module.
fn is_known_type(call_path: &CallPath, target_version: PythonVersion) -> bool {
    match call_path.as_slice() {
        ["" | "typing_extensions", ..] => true,
        [module, ..] => is_known_standard_library(target_version.minor(), module),
        _ => false,
    }
}

#[derive(Debug)]
enum TypingTarget<'a> {
    None,
    Any,
    Object,
    Optional,
    ForwardReference(Expr),
    Union(Vec<&'a Expr>),
    Literal(Vec<&'a Expr>),
    Annotated(&'a Expr),
}

impl<'a> TypingTarget<'a> {
    fn try_from_expr(
        expr: &'a Expr,
        semantic: &SemanticModel,
        locator: &Locator,
        target_version: PythonVersion,
    ) -> Option<Self> {
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if semantic.match_typing_expr(value, "Optional") {
                    return Some(TypingTarget::Optional);
                }
                let Expr::Tuple(ast::ExprTuple { elts: elements, .. }) = slice.as_ref() else {
                    return None;
                };
                if semantic.match_typing_expr(value, "Literal") {
                    Some(TypingTarget::Literal(elements.iter().collect()))
                } else if semantic.match_typing_expr(value, "Union") {
                    Some(TypingTarget::Union(elements.iter().collect()))
                } else if semantic.match_typing_expr(value, "Annotated") {
                    elements.first().map(TypingTarget::Annotated)
                } else {
                    semantic.resolve_call_path(value).map_or(
                        // If we can't resolve the call path, it must be defined
                        // in the same file, so we assume it's `Any` as it could
                        // be a type alias.
                        Some(TypingTarget::Any),
                        |call_path| {
                            if is_known_type(&call_path, target_version) {
                                None
                            } else {
                                // If it's not a known type, we assume it's `Any`.
                                Some(TypingTarget::Any)
                            }
                        },
                    )
                }
            }
            Expr::BinOp(..) => Some(TypingTarget::Union(
                PEP604UnionIterator::new(expr).collect(),
            )),
            Expr::Constant(ast::ExprConstant {
                value: Constant::None,
                ..
            }) => Some(TypingTarget::None),
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(string),
                range,
                ..
            }) => parse_type_annotation(string, *range, locator)
                // In case of a parse error, we return `Any` to avoid false positives.
                .map_or(Some(TypingTarget::Any), |(expr, _)| {
                    Some(TypingTarget::ForwardReference(expr))
                }),
            _ => semantic.resolve_call_path(expr).map_or(
                // If we can't resolve the call path, it must be defined in the
                // same file, so we assume it's `Any` as it could be a type alias.
                Some(TypingTarget::Any),
                |call_path| {
                    if semantic.match_typing_call_path(&call_path, "Any") {
                        Some(TypingTarget::Any)
                    } else if matches!(call_path.as_slice(), ["" | "builtins", "object"]) {
                        Some(TypingTarget::Object)
                    } else if !is_known_type(&call_path, target_version) {
                        // If it's not a known type, we assume it's `Any`.
                        Some(TypingTarget::Any)
                    } else {
                        None
                    }
                },
            ),
        }
    }

    /// Check if the [`TypingTarget`] explicitly allows `None`.
    fn contains_none(
        &self,
        semantic: &SemanticModel,
        locator: &Locator,
        target_version: PythonVersion,
    ) -> bool {
        match self {
            TypingTarget::None
            | TypingTarget::Optional
            | TypingTarget::Any
            | TypingTarget::Object => true,
            TypingTarget::Literal(elements) => elements.iter().any(|element| {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, target_version)
                else {
                    return false;
                };
                // Literal can only contain `None`, a literal value, other `Literal`
                // or an enum value.
                match new_target {
                    TypingTarget::None => true,
                    TypingTarget::Literal(_) => {
                        new_target.contains_none(semantic, locator, target_version)
                    }
                    _ => false,
                }
            }),
            TypingTarget::Union(elements) => elements.iter().any(|element| {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, target_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, target_version)
            }),
            TypingTarget::Annotated(element) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, target_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, target_version)
            }
            TypingTarget::ForwardReference(expr) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(expr, semantic, locator, target_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, target_version)
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
    annotation: &'a Expr,
    semantic: &SemanticModel,
    locator: &Locator,
    target_version: PythonVersion,
) -> Option<&'a Expr> {
    let Some(target) = TypingTarget::try_from_expr(annotation, semantic, locator, target_version)
    else {
        return Some(annotation);
    };
    match target {
        // Short circuit on top level `None`, `Any` or `Optional`
        TypingTarget::None | TypingTarget::Optional | TypingTarget::Any => None,
        // Top-level `Annotated` node should check for the inner type and
        // return the inner type if it doesn't allow `None`. If `Annotated`
        // is found nested inside another type, then the outer type should
        // be returned.
        TypingTarget::Annotated(expr) => {
            type_hint_explicitly_allows_none(expr, semantic, locator, target_version)
        }
        _ => {
            if target.contains_none(semantic, locator, target_version) {
                None
            } else {
                Some(annotation)
            }
        }
    }
}

/// Generate a [`Fix`] for the given [`Expr`] as per the [`ConversionType`].
fn generate_fix(checker: &Checker, conversion_type: ConversionType, expr: &Expr) -> Result<Fix> {
    match conversion_type {
        ConversionType::BinOpOr => {
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
            Ok(Fix::suggested(Edit::range_replacement(
                content,
                expr.range(),
            )))
        }
        ConversionType::Optional => {
            let (import_edit, binding) = checker.importer.get_or_import_symbol(
                &ImportRequest::import_from("typing", "Optional"),
                expr.start(),
                checker.semantic(),
            )?;
            let new_expr = Expr::Subscript(ast::ExprSubscript {
                range: TextRange::default(),
                value: Box::new(Expr::Name(ast::ExprName {
                    id: binding,
                    ctx: ast::ExprContext::Store,
                    range: TextRange::default(),
                })),
                slice: Box::new(expr.clone()),
                ctx: ast::ExprContext::Load,
            });
            let content = checker.generator().expr(&new_expr);
            Ok(Fix::suggested_edits(
                Edit::range_replacement(content, expr.range()),
                [import_edit],
            ))
        }
    }
}

/// RUF013
pub(crate) fn implicit_optional(checker: &mut Checker, arguments: &Arguments) {
    for ArgWithDefault {
        def,
        default,
        range: _,
    } in arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
    {
        let Some(default) = default else { continue };
        if !is_const_none(default) {
            continue;
        }
        let Some(annotation) = &def.annotation else {
            continue;
        };

        if let Expr::Constant(ast::ExprConstant {
            range,
            value: Constant::Str(string),
            ..
        }) = annotation.as_ref()
        {
            // Quoted annotation.
            if let Ok((annotation, kind)) = parse_type_annotation(string, *range, checker.locator) {
                let Some(expr) = type_hint_explicitly_allows_none(
                    &annotation,
                    checker.semantic(),
                    checker.locator,
                    checker.settings.target_version,
                ) else {
                    continue;
                };
                let conversion_type = checker.settings.target_version.into();

                let mut diagnostic =
                    Diagnostic::new(ImplicitOptional { conversion_type }, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    if kind.is_simple() {
                        diagnostic.try_set_fix(|| generate_fix(checker, conversion_type, expr));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        } else {
            // Unquoted annotation.
            let Some(expr) = type_hint_explicitly_allows_none(
                annotation,
                checker.semantic(),
                checker.locator,
                checker.settings.target_version,
            ) else {
                continue;
            };
            let conversion_type = checker.settings.target_version.into();

            let mut diagnostic =
                Diagnostic::new(ImplicitOptional { conversion_type }, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| generate_fix(checker, conversion_type, expr));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::call_path::CallPath;

    use crate::settings::types::PythonVersion;

    use super::is_known_type;

    #[test]
    fn test_is_known_type() {
        assert!(is_known_type(
            &CallPath::from_slice(&["", "int"]),
            PythonVersion::Py311
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["builtins", "int"]),
            PythonVersion::Py311
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["typing", "Optional"]),
            PythonVersion::Py311
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["typing_extensions", "Literal"]),
            PythonVersion::Py311
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["zoneinfo", "ZoneInfo"]),
            PythonVersion::Py311
        ));
        assert!(!is_known_type(
            &CallPath::from_slice(&["zoneinfo", "ZoneInfo"]),
            PythonVersion::Py38
        ));
    }
}
