//! Analysis rules to perform basic type inference on individual expressions.

use ruff_python_ast::ExprKind;
use rustc_hash::FxHashSet;

use ruff_python_ast::{self as ast, Expr, Operator, UnaryOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPythonType {
    /// The expression resolved to a single known type, like `str` or `int`.
    Atom(PythonType),
    /// The expression resolved to a union of known types, like `str | int`.
    Union(FxHashSet<PythonType>),
    /// The expression resolved to an unknown type, like a variable or function call.
    Unknown,
    /// The expression resolved to a `TypeError`, like `1 + "hello"`.
    TypeError,
}

impl ResolvedPythonType {
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::TypeError, _) | (_, Self::TypeError) => Self::TypeError,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Atom(a), Self::Atom(b)) => {
                if a.is_subtype_of(b) {
                    Self::Atom(b)
                } else if b.is_subtype_of(a) {
                    Self::Atom(a)
                } else {
                    Self::Union(FxHashSet::from_iter([a, b]))
                }
            }
            (Self::Atom(a), Self::Union(mut b)) => {
                // If `a` is a subtype of any of the types in `b`, then `a` is
                // redundant.
                if !b.iter().any(|b_element| a.is_subtype_of(*b_element)) {
                    b.insert(a);
                }
                Self::Union(b)
            }
            (Self::Union(mut a), Self::Atom(b)) => {
                // If `b` is a subtype of any of the types in `a`, then `b` is
                // redundant.
                if !a.iter().any(|a_element| b.is_subtype_of(*a_element)) {
                    a.insert(b);
                }
                Self::Union(a)
            }
            (Self::Union(mut a), Self::Union(b)) => {
                let mut b = b.into_iter().collect::<Vec<_>>();
                b.sort_unstable();
                for b_element in b {
                    // If `b_element` is a subtype of any of the types in `a`, then
                    // `b_element` is redundant.
                    if !a
                        .iter()
                        .any(|a_element| b_element.is_subtype_of(*a_element))
                    {
                        a.insert(b_element);
                    }
                }
                Self::Union(a)
            }
        }
    }
}

impl From<Expr<'_>> for ResolvedPythonType {
    fn from(expr: Expr<'_>) -> Self {
        match expr.classify() {
            // Primitives.
            ExprKind::Dict(_) => ResolvedPythonType::Atom(PythonType::Dict),
            ExprKind::DictComp(_) => ResolvedPythonType::Atom(PythonType::Dict),
            ExprKind::Set(_) => ResolvedPythonType::Atom(PythonType::Set),
            ExprKind::SetComp(_) => ResolvedPythonType::Atom(PythonType::Set),
            ExprKind::List(_) => ResolvedPythonType::Atom(PythonType::List),
            ExprKind::ListComp(_) => ResolvedPythonType::Atom(PythonType::List),
            ExprKind::Tuple(_) => ResolvedPythonType::Atom(PythonType::Tuple),
            ExprKind::Generator(_) => ResolvedPythonType::Atom(PythonType::Generator),
            ExprKind::FString(_) => ResolvedPythonType::Atom(PythonType::String),
            ExprKind::TString(_) => ResolvedPythonType::Unknown,
            ExprKind::StringLiteral(_) => ResolvedPythonType::Atom(PythonType::String),
            ExprKind::BytesLiteral(_) => ResolvedPythonType::Atom(PythonType::Bytes),
            ExprKind::NumberLiteral(expr_number_literal) => {
                let value = expr_number_literal.value();
                match value {
                    ast::Number::Int(_) => {
                        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
                    }
                    ast::Number::Float(_) => {
                        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
                    }
                    ast::Number::Complex { .. } => {
                        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Complex))
                    }
                }
            }
            ExprKind::BooleanLiteral(_) => {
                ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool))
            }
            ExprKind::NoneLiteral(_) => ResolvedPythonType::Atom(PythonType::None),
            ExprKind::EllipsisLiteral(_) => ResolvedPythonType::Atom(PythonType::Ellipsis),
            // Simple container expressions.
            ExprKind::Named(expr_named) => {
                let value = expr_named.value();
                ResolvedPythonType::from(value)
            }
            ExprKind::If(expr_if) => {
                let body = expr_if.body();
                let orelse = expr_if.orelse();

                let body = ResolvedPythonType::from(body);
                let orelse = ResolvedPythonType::from(orelse);
                body.union(orelse)
            }

            // Boolean operators.
            ExprKind::BoolOp(expr_bool_op) => {
                let values = expr_bool_op.values();
                values
                    .iter()
                    .map(ResolvedPythonType::from)
                    .reduce(ResolvedPythonType::union)
                    .unwrap_or(ResolvedPythonType::Unknown)
            }

            // Unary operators.
            ExprKind::UnaryOp(expr_unary_op) => {
                let operand = expr_unary_op.operand();
                let op = expr_unary_op.op();
                match op {
                    UnaryOp::Invert => match ResolvedPythonType::from(operand) {
                        ResolvedPythonType::Atom(PythonType::Number(
                            NumberLike::Bool | NumberLike::Integer,
                        )) => ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer)),
                        ResolvedPythonType::Atom(_) => ResolvedPythonType::TypeError,
                        _ => ResolvedPythonType::Unknown,
                    },
                    // Ex) `not 1.0`
                    UnaryOp::Not => ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool)),
                    // Ex) `+1` or `-1`
                    UnaryOp::UAdd | UnaryOp::USub => match ResolvedPythonType::from(operand) {
                        ResolvedPythonType::Atom(PythonType::Number(number)) => {
                            ResolvedPythonType::Atom(PythonType::Number(
                                if number == NumberLike::Bool {
                                    NumberLike::Integer
                                } else {
                                    number
                                },
                            ))
                        }
                        ResolvedPythonType::Atom(_) => ResolvedPythonType::TypeError,
                        _ => ResolvedPythonType::Unknown,
                    },
                }
            }

            // Binary operators.
            ExprKind::BinOp(expr_bin_op) => {
                let left = expr_bin_op.left();
                let op = expr_bin_op.op();
                let right = expr_bin_op.right();

                match op {
                    Operator::Add => {
                        match (
                            ResolvedPythonType::from(left),
                            ResolvedPythonType::from(right),
                        ) {
                            // Ex) `"Hello" + "world"`
                            (
                                ResolvedPythonType::Atom(PythonType::String),
                                ResolvedPythonType::Atom(PythonType::String),
                            ) => return ResolvedPythonType::Atom(PythonType::String),
                            // Ex) `b"Hello" + b"world"`
                            (
                                ResolvedPythonType::Atom(PythonType::Bytes),
                                ResolvedPythonType::Atom(PythonType::Bytes),
                            ) => return ResolvedPythonType::Atom(PythonType::Bytes),
                            // Ex) `[1] + [2]`
                            (
                                ResolvedPythonType::Atom(PythonType::List),
                                ResolvedPythonType::Atom(PythonType::List),
                            ) => return ResolvedPythonType::Atom(PythonType::List),
                            // Ex) `(1, 2) + (3, 4)`
                            (
                                ResolvedPythonType::Atom(PythonType::Tuple),
                                ResolvedPythonType::Atom(PythonType::Tuple),
                            ) => return ResolvedPythonType::Atom(PythonType::Tuple),
                            // Ex) `1 + 1.0`
                            (
                                ResolvedPythonType::Atom(PythonType::Number(left)),
                                ResolvedPythonType::Atom(PythonType::Number(right)),
                            ) => {
                                return ResolvedPythonType::Atom(PythonType::Number(
                                    left.coerce(right),
                                ));
                            }
                            // Ex) `"a" + 1`
                            (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                                return ResolvedPythonType::TypeError;
                            }
                            _ => {}
                        }
                    }
                    Operator::Sub => {
                        match (
                            ResolvedPythonType::from(left),
                            ResolvedPythonType::from(right),
                        ) {
                            // Ex) `1 - 1`
                            (
                                ResolvedPythonType::Atom(PythonType::Number(left)),
                                ResolvedPythonType::Atom(PythonType::Number(right)),
                            ) => {
                                return ResolvedPythonType::Atom(PythonType::Number(
                                    left.coerce(right),
                                ));
                            }
                            // Ex) `{1, 2} - {2}`
                            (
                                ResolvedPythonType::Atom(PythonType::Set),
                                ResolvedPythonType::Atom(PythonType::Set),
                            ) => return ResolvedPythonType::Atom(PythonType::Set),
                            // Ex) `"a" - "b"`
                            (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                                return ResolvedPythonType::TypeError;
                            }
                            _ => {}
                        }
                    }
                    // Ex) "a" % "b"
                    Operator::Mod => match (
                        ResolvedPythonType::from(left),
                        ResolvedPythonType::from(right),
                    ) {
                        // Ex) `"Hello" % "world"`
                        (ResolvedPythonType::Atom(PythonType::String), _) => {
                            return ResolvedPythonType::Atom(PythonType::String);
                        }
                        // Ex) `b"Hello" % b"world"`
                        (ResolvedPythonType::Atom(PythonType::Bytes), _) => {
                            return ResolvedPythonType::Atom(PythonType::Bytes);
                        }
                        // Ex) `1 % 2`
                        (
                            ResolvedPythonType::Atom(PythonType::Number(left)),
                            ResolvedPythonType::Atom(PythonType::Number(right)),
                        ) => {
                            return ResolvedPythonType::Atom(PythonType::Number(
                                left.coerce(right),
                            ));
                        }
                        _ => {}
                    },
                    Operator::Mult => match (
                        ResolvedPythonType::from(left),
                        ResolvedPythonType::from(right),
                    ) {
                        // Ex) `2 * 4`
                        (
                            ResolvedPythonType::Atom(PythonType::Number(left)),
                            ResolvedPythonType::Atom(PythonType::Number(right)),
                        ) => {
                            return ResolvedPythonType::Atom(PythonType::Number(
                                left.coerce(right),
                            ));
                        }
                        // Ex) `"1" * 2` or `2 * "1"`
                        (
                            ResolvedPythonType::Atom(PythonType::String),
                            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer)),
                        )
                        | (
                            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer)),
                            ResolvedPythonType::Atom(PythonType::String),
                        ) => return ResolvedPythonType::Atom(PythonType::String),
                        (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                            return ResolvedPythonType::TypeError;
                        }
                        _ => {}
                    },
                    // Standard arithmetic operators, which coerce to the "highest" number type.
                    Operator::FloorDiv | Operator::Pow => match (
                        ResolvedPythonType::from(left),
                        ResolvedPythonType::from(right),
                    ) {
                        // Ex) `2 ** 4`
                        (
                            ResolvedPythonType::Atom(PythonType::Number(left)),
                            ResolvedPythonType::Atom(PythonType::Number(right)),
                        ) => {
                            return ResolvedPythonType::Atom(PythonType::Number(
                                left.coerce(right),
                            ));
                        }
                        (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                            return ResolvedPythonType::TypeError;
                        }
                        _ => {}
                    },
                    // Division, which returns at least `float`.
                    Operator::Div => match (
                        ResolvedPythonType::from(left),
                        ResolvedPythonType::from(right),
                    ) {
                        // Ex) `1 / 2`
                        (
                            ResolvedPythonType::Atom(PythonType::Number(left)),
                            ResolvedPythonType::Atom(PythonType::Number(right)),
                        ) => {
                            let resolved = left.coerce(right);
                            return ResolvedPythonType::Atom(PythonType::Number(
                                if resolved == NumberLike::Integer {
                                    NumberLike::Float
                                } else {
                                    resolved
                                },
                            ));
                        }
                        (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                            return ResolvedPythonType::TypeError;
                        }
                        _ => {}
                    },
                    // Bitwise operators, which only work on `int` and `bool`.
                    Operator::BitAnd
                    | Operator::BitOr
                    | Operator::BitXor
                    | Operator::LShift
                    | Operator::RShift => {
                        match (
                            ResolvedPythonType::from(left),
                            ResolvedPythonType::from(right),
                        ) {
                            // Ex) `1 & 2`
                            (
                                ResolvedPythonType::Atom(PythonType::Number(left)),
                                ResolvedPythonType::Atom(PythonType::Number(right)),
                            ) => {
                                let resolved = left.coerce(right);
                                return if resolved == NumberLike::Integer {
                                    ResolvedPythonType::Atom(PythonType::Number(
                                        NumberLike::Integer,
                                    ))
                                } else {
                                    ResolvedPythonType::TypeError
                                };
                            }
                            (ResolvedPythonType::Atom(_), ResolvedPythonType::Atom(_)) => {
                                return ResolvedPythonType::TypeError;
                            }
                            _ => {}
                        }
                    }
                    Operator::MatMult => {}
                }
                ResolvedPythonType::Unknown
            }
            ExprKind::Lambda(_)
            | ExprKind::Await(_)
            | ExprKind::Yield(_)
            | ExprKind::YieldFrom(_)
            | ExprKind::Compare(_)
            | ExprKind::Call(_)
            | ExprKind::Attribute(_)
            | ExprKind::Subscript(_)
            | ExprKind::Starred(_)
            | ExprKind::Name(_)
            | ExprKind::Slice(_)
            | ExprKind::IpyEscapeCommand(_) => ResolvedPythonType::Unknown,
        }
    }
}

/// An extremely simple type inference system for individual expressions.
///
/// This system can only represent and infer the types of simple data types
/// such as strings, integers, floats, and containers. It cannot infer the
/// types of variables or expressions that are not statically known from
/// individual AST nodes alone.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PythonType {
    /// A string literal, such as `"hello"`.
    String,
    /// A bytes literal, such as `b"hello"`.
    Bytes,
    /// An integer, float, or complex literal, such as `1` or `1.0`.
    Number(NumberLike),
    /// A `None` literal, such as `None`.
    None,
    /// An ellipsis literal, such as `...`.
    Ellipsis,
    /// A dictionary literal, such as `{}` or `{"a": 1}`.
    Dict,
    /// A list literal, such as `[]` or `[i for i in range(3)]`.
    List,
    /// A set literal, such as `set()` or `{i for i in range(3)}`.
    Set,
    /// A tuple literal, such as `()` or `(1, 2, 3)`.
    Tuple,
    /// A generator expression, such as `(x for x in range(10))`.
    Generator,
}

impl PythonType {
    /// Returns `true` if `self` is a subtype of `other`.
    fn is_subtype_of(self, other: Self) -> bool {
        match (self, other) {
            (PythonType::String, PythonType::String) => true,
            (PythonType::Bytes, PythonType::Bytes) => true,
            (PythonType::None, PythonType::None) => true,
            (PythonType::Ellipsis, PythonType::Ellipsis) => true,
            // The Numeric Tower (https://peps.python.org/pep-3141/)
            (PythonType::Number(NumberLike::Bool), PythonType::Number(NumberLike::Bool)) => true,
            (PythonType::Number(NumberLike::Integer), PythonType::Number(NumberLike::Integer)) => {
                true
            }
            (PythonType::Number(NumberLike::Float), PythonType::Number(NumberLike::Float)) => true,
            (PythonType::Number(NumberLike::Complex), PythonType::Number(NumberLike::Complex)) => {
                true
            }
            (PythonType::Number(NumberLike::Bool), PythonType::Number(NumberLike::Integer)) => true,
            (PythonType::Number(NumberLike::Bool), PythonType::Number(NumberLike::Float)) => true,
            (PythonType::Number(NumberLike::Bool), PythonType::Number(NumberLike::Complex)) => true,
            (PythonType::Number(NumberLike::Integer), PythonType::Number(NumberLike::Float)) => {
                true
            }
            (PythonType::Number(NumberLike::Integer), PythonType::Number(NumberLike::Complex)) => {
                true
            }
            (PythonType::Number(NumberLike::Float), PythonType::Number(NumberLike::Complex)) => {
                true
            }
            // This simple type hierarchy doesn't support generics.
            (PythonType::Dict, PythonType::Dict) => true,
            (PythonType::List, PythonType::List) => true,
            (PythonType::Set, PythonType::Set) => true,
            (PythonType::Tuple, PythonType::Tuple) => true,
            (PythonType::Generator, PythonType::Generator) => true,
            _ => false,
        }
    }
}

/// A numeric type, or a type that can be trivially coerced to a numeric type.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NumberLike {
    /// An integer literal, such as `1` or `0x1`.
    Integer,
    /// A floating-point literal, such as `1.0` or `1e10`.
    Float,
    /// A complex literal, such as `1j` or `1+1j`.
    Complex,
    /// A boolean literal, such as `True` or `False`.
    Bool,
}

impl NumberLike {
    /// Coerces two number-like types to the "highest" number-like type.
    #[must_use]
    fn coerce(self, other: NumberLike) -> NumberLike {
        match (self, other) {
            (NumberLike::Complex, _) | (_, NumberLike::Complex) => NumberLike::Complex,
            (NumberLike::Float, _) | (_, NumberLike::Float) => NumberLike::Float,
            _ => NumberLike::Integer,
        }
    }
}

#[cfg(test)]
mod tests {

    use ruff_python_parser::{Parsed, parse_expression};

    use crate::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};

    fn parse(expression: &str) -> Parsed<ruff_python_ast::kind::ModExpression> {
        parse_expression(expression).unwrap()
    }

    #[test]
    fn type_inference() {
        // Atoms.
        assert_eq!(
            ResolvedPythonType::from(parse("1").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("'Hello, world'").expr()),
            ResolvedPythonType::Atom(PythonType::String)
        );
        assert_eq!(
            ResolvedPythonType::from(parse("b'Hello, world'").expr()),
            ResolvedPythonType::Atom(PythonType::Bytes)
        );
        assert_eq!(
            ResolvedPythonType::from(parse("'Hello' % 'world'").expr()),
            ResolvedPythonType::Atom(PythonType::String)
        );

        // Boolean operators.
        assert_eq!(
            ResolvedPythonType::from(parse("1 and 2").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1 and True").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );

        // Binary operators.
        assert_eq!(
            ResolvedPythonType::from(parse("1.0 * 2").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("2 * 1.0").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1.0 * 2j").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Complex))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("'AA' * 2").expr()),
            ResolvedPythonType::Atom(PythonType::String)
        );
        assert_eq!(
            ResolvedPythonType::from(parse("4 * 'AA'").expr()),
            ResolvedPythonType::Atom(PythonType::String)
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1 / True").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1 / 2").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("{1, 2} - {2}").expr()),
            ResolvedPythonType::Atom(PythonType::Set)
        );

        // Unary operators.
        assert_eq!(
            ResolvedPythonType::from(parse("-1").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("-1.0").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("-1j").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Complex))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("-True").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("not 'Hello'").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("not x.y.z").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool))
        );

        // Conditional expressions.
        assert_eq!(
            ResolvedPythonType::from(parse("1 if True else 2").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1 if True else 2.0").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float))
        );
        assert_eq!(
            ResolvedPythonType::from(parse("1 if True else False").expr()),
            ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer))
        );
    }
}
