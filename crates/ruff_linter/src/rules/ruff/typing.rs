use itertools::Either::{Left, Right};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Operator};

use ruff_python_stdlib::sys::is_known_standard_library;

use crate::checkers::ast::Checker;

/// Returns `true` if the given qualified name is a known type.
///
/// A known type is either a builtin type, any object from the standard library,
/// or a type from the `typing_extensions` module.
fn is_known_type(qualified_name: &QualifiedName, minor_version: u8) -> bool {
    match qualified_name.segments() {
        ["" | "typing_extensions", ..] => true,
        [module, ..] => is_known_standard_library(minor_version, module),
        _ => false,
    }
}

/// Returns an iterator over the expressions in a slice. If the slice is not a
/// tuple, the iterator will only yield the slice.
fn resolve_slice_value(slice: &Expr) -> impl Iterator<Item = &Expr> {
    match slice {
        Expr::Tuple(tuple) => Left(tuple.iter()),
        _ => Right(std::iter::once(slice)),
    }
}

#[derive(Debug)]
enum TypingTarget<'a> {
    /// Literal `None` type.
    None,

    /// A `typing.Any` type.
    Any,

    /// Literal `object` type.
    Object,

    /// Forward reference to a type e.g., `"List[str]"`.
    ForwardReference(&'a Expr),

    /// A `typing.Union` type e.g., `Union[int, str]`.
    Union(&'a Expr),

    /// A PEP 604 union type e.g., `int | str`.
    PEP604Union(&'a Expr, &'a Expr),

    /// A `typing.Literal` type e.g., `Literal[1, 2, 3]`.
    Literal(&'a Expr),

    /// A `typing.Optional` type e.g., `Optional[int]`.
    Optional(&'a Expr),

    /// A `typing.Annotated` type e.g., `Annotated[int, ...]`.
    Annotated(&'a Expr),

    /// The `typing.Hashable` type.
    Hashable,

    /// Special type used to represent an unknown type (and not a typing target)
    /// which could be a type alias.
    Unknown,

    /// Special type used to represent a known type (and not a typing target).
    /// A known type is either a builtin type, any object from the standard
    /// library, or a type from the `typing_extensions` module.
    Known,
}

impl<'a> TypingTarget<'a> {
    fn try_from_expr(expr: &'a Expr, checker: &'a Checker, minor_version: u8) -> Option<Self> {
        let semantic = checker.semantic();
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                semantic.resolve_qualified_name(value).map_or(
                    // If we can't resolve the call path, it must be defined
                    // in the same file and could be a type alias.
                    Some(TypingTarget::Unknown),
                    |qualified_name| {
                        if semantic.match_typing_qualified_name(&qualified_name, "Optional") {
                            Some(TypingTarget::Optional(slice.as_ref()))
                        } else if semantic.match_typing_qualified_name(&qualified_name, "Literal") {
                            Some(TypingTarget::Literal(slice.as_ref()))
                        } else if semantic.match_typing_qualified_name(&qualified_name, "Union") {
                            Some(TypingTarget::Union(slice.as_ref()))
                        } else if semantic.match_typing_qualified_name(&qualified_name, "Annotated")
                        {
                            resolve_slice_value(slice.as_ref())
                                .next()
                                .map(TypingTarget::Annotated)
                        } else {
                            if is_known_type(&qualified_name, minor_version) {
                                Some(TypingTarget::Known)
                            } else {
                                Some(TypingTarget::Unknown)
                            }
                        }
                    },
                )
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::BitOr,
                right,
                ..
            }) => Some(TypingTarget::PEP604Union(left, right)),
            Expr::NoneLiteral(_) => Some(TypingTarget::None),
            Expr::StringLiteral(string_expr) => checker
                .parse_type_annotation(string_expr)
                .as_ref()
                .map(|parsed_annotation| {
                    TypingTarget::ForwardReference(parsed_annotation.expression())
                }),
            _ => semantic.resolve_qualified_name(expr).map_or(
                // If we can't resolve the call path, it must be defined in the
                // same file, so we assume it's `Any` as it could be a type alias.
                Some(TypingTarget::Unknown),
                |qualified_name| {
                    if semantic.match_typing_qualified_name(&qualified_name, "Any") {
                        Some(TypingTarget::Any)
                    } else if matches!(qualified_name.segments(), ["" | "builtins", "object"]) {
                        Some(TypingTarget::Object)
                    } else if semantic.match_typing_qualified_name(&qualified_name, "Hashable")
                        || matches!(
                            qualified_name.segments(),
                            ["collections", "abc", "Hashable"]
                        )
                    {
                        Some(TypingTarget::Hashable)
                    } else if !is_known_type(&qualified_name, minor_version) {
                        // If it's not a known type, we assume it's `Any`.
                        Some(TypingTarget::Unknown)
                    } else {
                        Some(TypingTarget::Known)
                    }
                },
            ),
        }
    }

    /// Check if the [`TypingTarget`] explicitly allows `None`.
    fn contains_none(&self, checker: &Checker, minor_version: u8) -> bool {
        match self {
            TypingTarget::None
            | TypingTarget::Optional(_)
            | TypingTarget::Hashable
            | TypingTarget::Any
            | TypingTarget::Object
            | TypingTarget::Unknown => true,
            TypingTarget::Known => false,
            TypingTarget::Literal(slice) => resolve_slice_value(slice).any(|element| {
                // Literal can only contain `None`, a literal value, other `Literal`
                // or an enum value.
                match TypingTarget::try_from_expr(element, checker, minor_version) {
                    None | Some(TypingTarget::None) => true,
                    Some(new_target @ TypingTarget::Literal(_)) => {
                        new_target.contains_none(checker, minor_version)
                    }
                    _ => false,
                }
            }),
            TypingTarget::Union(slice) => resolve_slice_value(slice).any(|element| {
                TypingTarget::try_from_expr(element, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_none(checker, minor_version)
                    })
            }),
            TypingTarget::PEP604Union(left, right) => [left, right].iter().any(|element| {
                TypingTarget::try_from_expr(element, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_none(checker, minor_version)
                    })
            }),
            TypingTarget::Annotated(expr) => {
                TypingTarget::try_from_expr(expr, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_none(checker, minor_version)
                    })
            }
            TypingTarget::ForwardReference(expr) => {
                TypingTarget::try_from_expr(expr, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_none(checker, minor_version)
                    })
            }
        }
    }

    /// Check if the [`TypingTarget`] explicitly allows `Any`.
    fn contains_any(&self, checker: &Checker, minor_version: u8) -> bool {
        match self {
            TypingTarget::Any => true,
            // `Literal` cannot contain `Any` as it's a dynamic value.
            TypingTarget::Literal(_)
            | TypingTarget::None
            | TypingTarget::Hashable
            | TypingTarget::Object
            | TypingTarget::Known
            | TypingTarget::Unknown => false,
            TypingTarget::Union(slice) => resolve_slice_value(slice).any(|element| {
                TypingTarget::try_from_expr(element, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_any(checker, minor_version)
                    })
            }),
            TypingTarget::PEP604Union(left, right) => [left, right].iter().any(|element| {
                TypingTarget::try_from_expr(element, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_any(checker, minor_version)
                    })
            }),
            TypingTarget::Annotated(expr) | TypingTarget::Optional(expr) => {
                TypingTarget::try_from_expr(expr, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_any(checker, minor_version)
                    })
            }
            TypingTarget::ForwardReference(expr) => {
                TypingTarget::try_from_expr(expr, checker, minor_version)
                    .map_or(true, |new_target| {
                        new_target.contains_any(checker, minor_version)
                    })
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
    checker: &'a Checker,
    minor_version: u8,
) -> Option<&'a Expr> {
    match TypingTarget::try_from_expr(annotation, checker, minor_version) {
        None |
            // Short circuit on top level `None`, `Any` or `Optional`
            Some(TypingTarget::None | TypingTarget::Optional(_) | TypingTarget::Any) => None,
        // Top-level `Annotated` node should check for the inner type and
        // return the inner type if it doesn't allow `None`. If `Annotated`
        // is found nested inside another type, then the outer type should
        // be returned.
        Some(TypingTarget::Annotated(expr)) => {
            type_hint_explicitly_allows_none(expr, checker, minor_version)
        }
        Some(target) => {
            if target.contains_none(checker, minor_version) {
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
    checker: &Checker,
    minor_version: u8,
) -> bool {
    match TypingTarget::try_from_expr(annotation, checker, minor_version) {
        None |
            // Short circuit on top level `Any`
            Some(TypingTarget::Any) => true,
        // Top-level `Annotated` node should check if the inner type resolves
        // to `Any`.
        Some(TypingTarget::Annotated(expr)) => {
            type_hint_resolves_to_any(expr, checker, minor_version)
        }
        Some(target) => target.contains_any(checker, minor_version),
    }
}

#[cfg(test)]
mod tests {
    use super::is_known_type;
    use ruff_python_ast::name::QualifiedName;

    #[test]
    fn test_is_known_type() {
        assert!(is_known_type(&QualifiedName::builtin("int"), 11));
        assert!(is_known_type(
            &QualifiedName::from_iter(["builtins", "int"]),
            11
        ));
        assert!(is_known_type(
            &QualifiedName::from_iter(["typing", "Optional"]),
            11
        ));
        assert!(is_known_type(
            &QualifiedName::from_iter(["typing_extensions", "Literal"]),
            11
        ));
        assert!(is_known_type(
            &QualifiedName::from_iter(["zoneinfo", "ZoneInfo"]),
            11
        ));
        assert!(!is_known_type(
            &QualifiedName::from_iter(["zoneinfo", "ZoneInfo"]),
            8
        ));
    }
}
