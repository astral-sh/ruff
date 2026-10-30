use crate::flat::{self, Any, View, fields as ast};
use crate::helpers::{Truthiness, is_iterable_initializer};
use crate::{ExceptHandler, Expr, ExprFString, InterpolatedStringElement, Number};

/// Tests expressions in the subtree, stopping at the first match.
pub fn any_over_expr<'a>(
    expr: View<'a, Expr>,
    predicate: impl FnMut(View<'a, Expr>) -> bool,
) -> bool {
    any_expression(expr.erase(), predicate)
}

pub(crate) fn any_expression<'a>(
    root: View<'a, Any>,
    predicate: impl FnMut(View<'a, Expr>) -> bool,
) -> bool {
    root.descendants()
        .filter_map(View::cast::<Expr>)
        .any(predicate)
}

impl Truthiness {
    /// Return the truthiness of an expression.
    pub fn from_view<F>(expr: flat::View<'_, Expr>, is_builtin: F) -> Self
    where
        F: Fn(&str) -> bool,
    {
        match expr.fields() {
            ast::Expr::Lambda(_) => Self::Truthy,
            ast::Expr::Generator(_) => Self::Truthy,
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            ast::Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => match value {
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
            },
            ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => {
                if value {
                    Self::True
                } else {
                    Self::False
                }
            }
            ast::Expr::NoneLiteral(_) => Self::None,
            ast::Expr::EllipsisLiteral(_) => Self::Truthy,
            ast::Expr::FString(ast::ExprFString { node: f_string, .. }) => {
                if is_empty_f_string(f_string) {
                    Self::Falsey
                } else if is_non_empty_f_string(f_string) {
                    Self::Truthy
                } else {
                    Self::Unknown
                }
            }
            ast::Expr::TString(_) => Self::Truthy,
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Set(ast::ExprSet { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                if elts.is_empty() {
                    return Self::Falsey;
                }

                if elts.iter().all(super::View::is_starred_expr) {
                    // [*foo] / [*foo, *bar]
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            ast::Expr::Dict(ast::ExprDict { node: dict, .. }) => {
                if dict.items().is_empty() {
                    return Self::Falsey;
                }

                // If the dict consists only of double-starred items (e.g., {**x, **y}),
                // consider its truthiness unknown. This matches lists/sets/tuples containing
                // only starred elements, which are also Unknown.
                if dict.items().iter().all(|item| item.key.is_none()) {
                    // {**foo} / {**foo, **bar}
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            ast::Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                if let ast::Expr::Name(ast::ExprName { id, .. }) = func.fields() {
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
                                _ => Self::from_view(argument, is_builtin),
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
pub fn is_empty_f_string(expr: flat::View<'_, ExprFString>) -> bool {
    fn inner(expr: flat::View<'_, Expr>) -> bool {
        match expr.fields() {
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
            // Confusingly, `bool(f"{b""}") == True` even though
            // `bool(b"") == False`. This is because `f"{b""}"`
            // evaluates as the string `'b""'` of length 3.
            ast::Expr::BytesLiteral(_) => false,
            ast::Expr::FString(ast::ExprFString { value, .. }) => {
                is_empty_interpolated_elements(value.elements())
            }
            _ => false,
        }
    }

    fn is_empty_interpolated_elements<'a>(
        mut elements: impl Iterator<Item = flat::View<'a, InterpolatedStringElement>>,
    ) -> bool {
        elements.all(|element| match element.fields() {
            ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
                value,
                ..
            }) => value.is_empty(),
            ast::InterpolatedStringElement::Interpolation(ast::InterpolatedElement {
                node: f_string,
                ..
            }) => {
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

fn is_non_empty_f_string(expr: flat::View<'_, ExprFString>) -> bool {
    fn inner(expr: flat::View<'_, Expr>) -> bool {
        match expr.fields() {
            // When stringified, these expressions are always non-empty.
            ast::Expr::Lambda(_) => true,
            ast::Expr::Dict(_) => true,
            ast::Expr::Set(_) => true,
            ast::Expr::ListComp(_) => true,
            ast::Expr::SetComp(_) => true,
            ast::Expr::DictComp(_) => true,
            ast::Expr::NumberLiteral(_) => true,
            ast::Expr::BooleanLiteral(_) => true,
            ast::Expr::NoneLiteral(_) => true,
            ast::Expr::EllipsisLiteral(_) => true,
            ast::Expr::List(_) => true,
            ast::Expr::Tuple(_) => true,
            ast::Expr::TString(_) => true,

            // These expressions must resolve to the inner expression.
            ast::Expr::If(ast::ExprIf { body, orelse, .. }) => inner(body) && inner(orelse),
            ast::Expr::Named(ast::ExprNamed { value, .. }) => inner(value),

            // These expressions are complex. We can't determine whether they're empty or not.
            ast::Expr::BoolOp(ast::ExprBoolOp { .. }) => false,
            ast::Expr::BinOp(ast::ExprBinOp { .. }) => false,
            ast::Expr::UnaryOp(ast::ExprUnaryOp { .. }) => false,
            // Rich comparison methods can return arbitrary objects.
            ast::Expr::Compare(_) => false,
            ast::Expr::Generator(_) => false,
            ast::Expr::Await(_) => false,
            ast::Expr::Yield(_) => false,
            ast::Expr::YieldFrom(_) => false,
            ast::Expr::Call(_) => false,
            ast::Expr::Attribute(_) => false,
            ast::Expr::Subscript(_) => false,
            ast::Expr::Starred(_) => false,
            ast::Expr::Name(_) => false,
            ast::Expr::Slice(_) => false,
            ast::Expr::IpyEscapeCommand(_) => false,

            // These literals may or may not be empty.
            ast::Expr::FString(ast::ExprFString { node: f_string, .. }) => {
                is_non_empty_f_string(f_string)
            }
            // These literals may or may not be empty.
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => !value.is_empty(),
            // Confusingly, f"{b""}" renders as the string 'b""', which is non-empty.
            // Therefore, any bytes interpolation is guaranteed non-empty when stringified.
            ast::Expr::BytesLiteral(_) => true,
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

pub fn is_dotted_name(expr: flat::View<'_, Expr>) -> bool {
    match expr.fields() {
        ast::Expr::Name(_) => true,
        ast::Expr::Attribute(ast::ExprAttribute { value, .. }) => is_dotted_name(value),
        _ => false,
    }
}

pub fn is_const_true(expr: flat::View<'_, Expr>) -> bool {
    expr.as_boolean_literal_expr()
        .is_some_and(View::<crate::ExprBooleanLiteral>::value)
}

pub fn is_const_false(expr: flat::View<'_, Expr>) -> bool {
    expr.as_boolean_literal_expr()
        .is_some_and(|boolean| !boolean.value())
}

pub fn map_callable(decorator: flat::View<'_, Expr>) -> flat::View<'_, Expr> {
    if let Some(call) = decorator.as_call_expr() {
        // Ex) `@decorator()`
        call.func()
    } else {
        // Ex) `@decorator`
        decorator
    }
}

pub fn map_subscript(expr: flat::View<'_, Expr>) -> flat::View<'_, Expr> {
    if let Some(subscript) = expr.as_subscript_expr() {
        // Ex) `Iterable[T]`  => return `Iterable`
        subscript.value()
    } else {
        // Ex) `Iterable`  => return `Iterable`
        expr
    }
}

pub fn extract_handled_exceptions(
    handlers: flat::NodeList<'_, ExceptHandler>,
) -> Vec<flat::View<'_, Expr>> {
    let mut handled_exceptions = Vec::new();
    for handler in handlers {
        match handler.fields() {
            ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                type_, ..
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
