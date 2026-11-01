use crate as ast;
use crate::flat::{self, AstNode};
use crate::helpers::{Truthiness, is_iterable_initializer};
use crate::{Expr, ExprFString, InterpolatedStringElement, Number};

/// Tests expressions in the subtree, stopping at the first match.
pub fn any_over_expr<'a>(expr: Expr<'a>, predicate: impl FnMut(Expr<'a>) -> bool) -> bool {
    any_expression(expr.syntax_node(), predicate)
}

pub(crate) fn any_expression<'a>(
    root: crate::SyntaxNode<'a>,
    predicate: impl FnMut(Expr<'a>) -> bool,
) -> bool {
    root.descendants()
        .filter_map(AstNode::cast::<Expr>)
        .any(predicate)
}

impl Truthiness {
    /// Return the truthiness of an expression.
    pub fn from_expr<F>(expr: Expr<'_>, is_builtin: F) -> Self
    where
        F: Fn(&str) -> bool,
    {
        match expr.fields() {
            ast::ExprFields::Lambda(_) => Self::Truthy,
            ast::ExprFields::Generator(_) => Self::Truthy,
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            ast::ExprFields::BytesLiteral(ast::ExprBytesLiteralFields { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields { value, .. }) => {
                match value {
                    Number::Int(int) => {
                        if *int == 0 {
                            Self::Falsey
                        } else {
                            Self::Truthy
                        }
                    }
                    Number::Float(float) => {
                        if *float == 0.0 {
                            Self::Falsey
                        } else {
                            Self::Truthy
                        }
                    }
                    Number::Complex { real, imag, .. } => {
                        if *real == 0.0 && *imag == 0.0 {
                            Self::Falsey
                        } else {
                            Self::Truthy
                        }
                    }
                }
            }
            ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields { value, .. }) => {
                if value { Self::True } else { Self::False }
            }
            ast::ExprFields::NoneLiteral(_) => Self::None,
            ast::ExprFields::EllipsisLiteral(_) => Self::Truthy,
            ast::ExprFields::FString(ast::ExprFStringFields { node: f_string, .. }) => {
                if is_empty_f_string(f_string) {
                    Self::Falsey
                } else if is_non_empty_f_string(f_string) {
                    Self::Truthy
                } else {
                    Self::Unknown
                }
            }
            ast::ExprFields::TString(_) => Self::Truthy,
            ast::ExprFields::List(ast::ExprListFields { elts, .. })
            | ast::ExprFields::Set(ast::ExprSetFields { elts, .. })
            | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
                if elts.is_empty() {
                    return Self::Falsey;
                }

                if elts
                    .iter()
                    .all(super::super::generated::Expr::is_starred_expr)
                {
                    // [*foo] / [*foo, *bar]
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            ast::ExprFields::Dict(ast::ExprDictFields { node: dict, .. }) => {
                if dict.items().is_empty() {
                    return Self::Falsey;
                }

                // If the dict consists only of double-starred items (e.g., {**x, **y}),
                // consider its truthiness unknown. This matches lists/sets/tuples containing
                // only starred elements, which are also Unknown.
                if dict.items().iter().all(|item| item.key().is_none()) {
                    // {**foo} / {**foo, **bar}
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            ast::ExprFields::Call(ast::ExprCallFields {
                func, arguments, ..
            }) => {
                if let ast::ExprFields::Name(ast::ExprNameFields { id, .. }) = func.fields() {
                    if is_iterable_initializer(id.as_str(), |id| is_builtin(id)) {
                        if arguments.is_empty() {
                            // Ex) `list()`
                            Self::Falsey
                        } else if let Some([argument]) = arguments.args().as_array()
                            && arguments.keywords().is_empty()
                        {
                            // Ex) `list([1, 2, 3])`
                            match argument.classify() {
                                // Return Unknown for types with definite truthiness that might
                                // result in empty iterables (t-strings and generators) or will
                                // raise a type error (non-iterable types like numbers, booleans,
                                // None, etc.).
                                flat::ExprKind::NumberLiteral(_)
                                | flat::ExprKind::BooleanLiteral(_)
                                | flat::ExprKind::NoneLiteral(_)
                                | flat::ExprKind::EllipsisLiteral(_)
                                | flat::ExprKind::TString(_)
                                | flat::ExprKind::Lambda(_)
                                | flat::ExprKind::Generator(_) => Self::Unknown,
                                // Recurse for all other types - collections, comprehensions, variables, etc.
                                // StringLiteral, FString, and BytesLiteral recurse because Self::from_expr
                                // correctly handles their truthiness (checking if empty or not).
                                _ => Self::from_expr(argument, is_builtin),
                            }
                        } else {
                            Self::Unknown
                        }
                    } else {
                        Self::Unknown
                    }
                } else {
                    Self::Unknown
                }
            }
            _ => Self::Unknown,
        }
    }
}
pub fn is_empty_f_string(expr: ExprFString<'_>) -> bool {
    fn inner(expr: Expr<'_>) -> bool {
        match expr.fields() {
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields { value, .. }) => {
                value.is_empty()
            }
            // Confusingly, `bool(f"{b""}") == True` even though
            // `bool(b"") == False`. This is because `f"{b""}"`
            // evaluates as the string `'b""'` of length 3.
            ast::ExprFields::BytesLiteral(_) => false,
            ast::ExprFields::FString(ast::ExprFStringFields { value, .. }) => {
                is_empty_interpolated_elements(value.elements())
            }
            _ => false,
        }
    }

    fn is_empty_interpolated_elements<'a>(
        mut elements: impl Iterator<Item = InterpolatedStringElement<'a>>,
    ) -> bool {
        elements.all(|element| match element.fields() {
            ast::InterpolatedStringElementFields::Literal(
                ast::InterpolatedStringLiteralElementFields { value, .. },
            ) => value.is_empty(),
            ast::InterpolatedStringElementFields::Interpolation(
                ast::InterpolatedElementFields { node: f_string, .. },
            ) => {
                f_string.debug_text().is_none()
                    && f_string.conversion().is_none()
                    && f_string.format_spec().is_none()
                    && inner(f_string.expression())
            }
        })
    }

    expr.value().iter().all(|part| match part {
        flat::FStringPartView::Literal(string_literal) => string_literal.value().is_empty(),
        flat::FStringPartView::FString(f_string) => {
            is_empty_interpolated_elements(f_string.elements().iter())
        }
    })
}

fn is_non_empty_f_string(expr: ExprFString<'_>) -> bool {
    fn inner(expr: Expr<'_>) -> bool {
        match expr.fields() {
            // When stringified, these expressions are always non-empty.
            ast::ExprFields::Lambda(_) => true,
            ast::ExprFields::Dict(_) => true,
            ast::ExprFields::Set(_) => true,
            ast::ExprFields::ListComp(_) => true,
            ast::ExprFields::SetComp(_) => true,
            ast::ExprFields::DictComp(_) => true,
            ast::ExprFields::NumberLiteral(_) => true,
            ast::ExprFields::BooleanLiteral(_) => true,
            ast::ExprFields::NoneLiteral(_) => true,
            ast::ExprFields::EllipsisLiteral(_) => true,
            ast::ExprFields::List(_) => true,
            ast::ExprFields::Tuple(_) => true,
            ast::ExprFields::TString(_) => true,

            // These expressions must resolve to the inner expression.
            ast::ExprFields::If(ast::ExprIfFields { body, orelse, .. }) => {
                inner(body) && inner(orelse)
            }
            ast::ExprFields::Named(ast::ExprNamedFields { value, .. }) => inner(value),

            // These expressions are complex. We can't determine whether they're empty or not.
            ast::ExprFields::BoolOp(ast::ExprBoolOpFields { .. }) => false,
            ast::ExprFields::BinOp(ast::ExprBinOpFields { .. }) => false,
            ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields { .. }) => false,
            // Rich comparison methods can return arbitrary objects.
            ast::ExprFields::Compare(_) => false,
            ast::ExprFields::Generator(_) => false,
            ast::ExprFields::Await(_) => false,
            ast::ExprFields::Yield(_) => false,
            ast::ExprFields::YieldFrom(_) => false,
            ast::ExprFields::Call(_) => false,
            ast::ExprFields::Attribute(_) => false,
            ast::ExprFields::Subscript(_) => false,
            ast::ExprFields::Starred(_) => false,
            ast::ExprFields::Name(_) => false,
            ast::ExprFields::Slice(_) => false,
            ast::ExprFields::IpyEscapeCommand(_) => false,

            // These literals may or may not be empty.
            ast::ExprFields::FString(ast::ExprFStringFields { node: f_string, .. }) => {
                is_non_empty_f_string(f_string)
            }
            // These literals may or may not be empty.
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields { value, .. }) => {
                !value.is_empty()
            }
            // Confusingly, f"{b""}" renders as the string 'b""', which is non-empty.
            // Therefore, any bytes interpolation is guaranteed non-empty when stringified.
            ast::ExprFields::BytesLiteral(_) => true,
        }
    }

    expr.value().iter().any(|part| match part {
        flat::FStringPartView::Literal(string_literal) => !string_literal.value().is_empty(),
        flat::FStringPartView::FString(f_string) => {
            // The part is a concatenation of elements, so it's guaranteed non-empty if any element is
            f_string
                .elements()
                .iter()
                .any(|element| match element.classify() {
                    flat::InterpolatedStringElementKind::Literal(string_literal) => {
                        !string_literal.value().is_empty()
                    }
                    flat::InterpolatedStringElementKind::Interpolation(f_string) => {
                        f_string.debug_text().is_some()
                            || (f_string.format_spec().is_none() && inner(f_string.expression()))
                    }
                })
        }
    })
}

pub fn is_dotted_name(expr: Expr<'_>) -> bool {
    match expr.fields() {
        ast::ExprFields::Name(_) => true,
        ast::ExprFields::Attribute(ast::ExprAttributeFields { value, .. }) => is_dotted_name(value),
        _ => false,
    }
}

pub fn is_const_true(expr: Expr<'_>) -> bool {
    expr.as_boolean_literal_expr()
        .is_some_and(crate::ExprBooleanLiteral::value)
}

pub fn is_const_false(expr: Expr<'_>) -> bool {
    expr.as_boolean_literal_expr()
        .is_some_and(|boolean| !boolean.value())
}

pub fn map_callable(decorator: Expr<'_>) -> Expr<'_> {
    if let Some(call) = decorator.as_call_expr() {
        // Ex) `@decorator()`
        call.func()
    } else {
        // Ex) `@decorator`
        decorator
    }
}

pub fn map_subscript(expr: Expr<'_>) -> Expr<'_> {
    if let Some(subscript) = expr.as_subscript_expr() {
        // Ex) `Iterable[T]`  => return `Iterable`
        subscript.value()
    } else {
        // Ex) `Iterable`  => return `Iterable`
        expr
    }
}

pub fn extract_handled_exceptions(
    handlers: flat::NodeList<'_, crate::kind::ExceptHandler>,
) -> Vec<Expr<'_>> {
    let mut handled_exceptions = Vec::new();
    for handler in handlers {
        match handler.fields() {
            ast::ExceptHandlerFields::ExceptHandler(ast::ExceptHandlerExceptHandlerFields {
                type_,
                ..
            }) => {
                if let Some(type_) = type_ {
                    if let flat::ExprKind::Tuple(tuple) = type_.classify() {
                        for type_ in tuple.elts() {
                            handled_exceptions.push(type_);
                        }
                    } else {
                        handled_exceptions.push(type_);
                    }
                }
            }
        }
    }
    handled_exceptions
}
