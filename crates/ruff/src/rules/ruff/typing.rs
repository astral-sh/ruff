use rustpython_parser::ast::{self, Constant, Expr, Operator};

use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::typing::parse_type_annotation;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::sys::is_known_standard_library;

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
fn is_known_type(call_path: &CallPath, minor_version: u32) -> bool {
    match call_path.as_slice() {
        ["" | "typing_extensions", ..] => true,
        [module, ..] => is_known_standard_library(minor_version, module),
        _ => false,
    }
}

#[derive(Debug)]
enum TypingTarget<'a> {
    None,
    Any,
    Object,
    ForwardReference(Expr),
    Union(Vec<&'a Expr>),
    Literal(Vec<&'a Expr>),
    Optional(&'a Expr),
    Annotated(&'a Expr),
}

impl<'a> TypingTarget<'a> {
    fn try_from_expr(
        expr: &'a Expr,
        semantic: &SemanticModel,
        locator: &Locator,
        minor_version: u32,
    ) -> Option<Self> {
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if semantic.match_typing_expr(value, "Optional") {
                    return Some(TypingTarget::Optional(slice.as_ref()));
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
                            if is_known_type(&call_path, minor_version) {
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
                    } else if !is_known_type(&call_path, minor_version) {
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
        minor_version: u32,
    ) -> bool {
        match self {
            TypingTarget::None
            | TypingTarget::Optional(_)
            | TypingTarget::Any
            | TypingTarget::Object => true,
            TypingTarget::Literal(elements) => elements.iter().any(|element| {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, minor_version)
                else {
                    return false;
                };
                // Literal can only contain `None`, a literal value, other `Literal`
                // or an enum value.
                match new_target {
                    TypingTarget::None => true,
                    TypingTarget::Literal(_) => {
                        new_target.contains_none(semantic, locator, minor_version)
                    }
                    _ => false,
                }
            }),
            TypingTarget::Union(elements) => elements.iter().any(|element| {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, minor_version)
            }),
            TypingTarget::Annotated(element) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, minor_version)
            }
            TypingTarget::ForwardReference(expr) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(expr, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_none(semantic, locator, minor_version)
            }
        }
    }

    /// Check if the [`TypingTarget`] explicitly allows `Any`.
    fn contains_any(
        &self,
        semantic: &SemanticModel,
        locator: &Locator,
        minor_version: u32,
    ) -> bool {
        match self {
            TypingTarget::Any => true,
            // `Literal` cannot contain `Any` as it's a dynamic value.
            TypingTarget::Literal(_) | TypingTarget::None | TypingTarget::Object => false,
            TypingTarget::Union(elements) => elements.iter().any(|element| {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_any(semantic, locator, minor_version)
            }),
            TypingTarget::Annotated(element) | TypingTarget::Optional(element) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(element, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_any(semantic, locator, minor_version)
            }
            TypingTarget::ForwardReference(expr) => {
                let Some(new_target) =
                    TypingTarget::try_from_expr(expr, semantic, locator, minor_version)
                else {
                    return false;
                };
                new_target.contains_any(semantic, locator, minor_version)
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
pub(crate) fn type_hint_explicitly_allows_none<'a>(
    annotation: &'a Expr,
    semantic: &SemanticModel,
    locator: &Locator,
    minor_version: u32,
) -> Option<&'a Expr> {
    let Some(target) = TypingTarget::try_from_expr(annotation, semantic, locator, minor_version)
    else {
        return Some(annotation);
    };
    match target {
        // Short circuit on top level `None`, `Any` or `Optional`
        TypingTarget::None | TypingTarget::Optional(_) | TypingTarget::Any => None,
        // Top-level `Annotated` node should check for the inner type and
        // return the inner type if it doesn't allow `None`. If `Annotated`
        // is found nested inside another type, then the outer type should
        // be returned.
        TypingTarget::Annotated(expr) => {
            type_hint_explicitly_allows_none(expr, semantic, locator, minor_version)
        }
        _ => {
            if target.contains_none(semantic, locator, minor_version) {
                None
            } else {
                Some(annotation)
            }
        }
    }
}

/// Check if the given annotation [`Expr`] resolves to `Any`.
///
/// This function assumes that the annotation is a valid typing annotation expression.
pub(crate) fn type_hint_resolves_to_any(
    annotation: &Expr,
    semantic: &SemanticModel,
    locator: &Locator,
    minor_version: u32,
) -> bool {
    let Some(target) = TypingTarget::try_from_expr(annotation, semantic, locator, minor_version)
    else {
        return false;
    };
    match target {
        // Short circuit on top level `Any`
        TypingTarget::Any => true,
        // Top-level `Annotated` node should check if the inner type resolves
        // to `Any`.
        TypingTarget::Annotated(expr) => {
            type_hint_resolves_to_any(expr, semantic, locator, minor_version)
        }
        _ => target.contains_any(semantic, locator, minor_version),
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::call_path::CallPath;

    use super::is_known_type;

    #[test]
    fn test_is_known_type() {
        assert!(is_known_type(&CallPath::from_slice(&["", "int"]), 11));
        assert!(is_known_type(
            &CallPath::from_slice(&["builtins", "int"]),
            11
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["typing", "Optional"]),
            11
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["typing_extensions", "Literal"]),
            11
        ));
        assert!(is_known_type(
            &CallPath::from_slice(&["zoneinfo", "ZoneInfo"]),
            11
        ));
        assert!(!is_known_type(
            &CallPath::from_slice(&["zoneinfo", "ZoneInfo"]),
            8
        ));
    }
}
