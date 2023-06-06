//! Analysis rules to perform basic type inference on individual expressions.

use rustpython_parser::ast;
use rustpython_parser::ast::{Constant, Expr};

/// An extremely simple type inference system for individual expressions.
///
/// This system can only represent and infer the types of simple data types
/// such as strings, integers, floats, and containers. It cannot infer the
/// types of variables or expressions that are not statically known from
/// individual AST nodes alone.
#[derive(Debug, Copy, Clone)]
pub enum PythonType {
    /// A string literal, such as `"hello"`.
    String,
    /// A bytes literal, such as `b"hello"`.
    Bytes,
    /// An integer literal, such as `1` or `0x1`.
    Integer,
    /// A floating-point literal, such as `1.0` or `1e10`.
    Float,
    /// A complex literal, such as `1j` or `1+1j`.
    Complex,
    /// A boolean literal, such as `True` or `False`.
    Bool,
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
    /// An unknown type, such as a variable or function call.
    Unknown,
}

impl From<&Expr> for PythonType {
    fn from(expr: &Expr) -> Self {
        match expr {
            Expr::NamedExpr(ast::ExprNamedExpr { value, .. }) => (&**value).into(),
            Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => (&**operand).into(),
            Expr::Dict(_) => PythonType::Dict,
            Expr::DictComp(_) => PythonType::Dict,
            Expr::Set(_) => PythonType::Set,
            Expr::SetComp(_) => PythonType::Set,
            Expr::List(_) => PythonType::List,
            Expr::ListComp(_) => PythonType::List,
            Expr::Tuple(_) => PythonType::Tuple,
            Expr::GeneratorExp(_) => PythonType::Generator,
            Expr::JoinedStr(_) => PythonType::String,
            Expr::BinOp(ast::ExprBinOp { left, op, .. }) => {
                // Ex) "a" % "b"
                if op.is_mod() {
                    if matches!(
                        left.as_ref(),
                        Expr::Constant(ast::ExprConstant {
                            value: Constant::Str(..),
                            ..
                        })
                    ) {
                        return PythonType::String;
                    }
                    if matches!(
                        left.as_ref(),
                        Expr::Constant(ast::ExprConstant {
                            value: Constant::Bytes(..),
                            ..
                        })
                    ) {
                        return PythonType::Bytes;
                    }
                }
                PythonType::Unknown
            }
            Expr::Constant(ast::ExprConstant { value, .. }) => match value {
                Constant::Str(_) => PythonType::String,
                Constant::Int(_) => PythonType::Integer,
                Constant::Float(_) => PythonType::Float,
                Constant::Bool(_) => PythonType::Bool,
                Constant::Complex { .. } => PythonType::Complex,
                Constant::None => PythonType::None,
                Constant::Ellipsis => PythonType::Ellipsis,
                Constant::Bytes(_) => PythonType::Bytes,
                Constant::Tuple(_) => PythonType::Tuple,
            },
            _ => PythonType::Unknown,
        }
    }
}
