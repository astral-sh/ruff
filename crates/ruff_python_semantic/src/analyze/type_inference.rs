//! Analysis rules to perform basic type inference on individual expressions.

use ruff_python_ast as ast;
use ruff_python_ast::{Constant, Expr, Operator};

/// An extremely simple type inference system for individual expressions.
///
/// This system can only represent and infer the types of simple data types
/// such as strings, integers, floats, and containers. It cannot infer the
/// types of variables or expressions that are not statically known from
/// individual AST nodes alone.
#[derive(Debug, Copy, Clone, PartialEq, Eq, is_macro::Is)]
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
            Expr::NamedExpr(ast::ExprNamedExpr { value, .. }) => (value.as_ref()).into(),
            Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => (operand.as_ref()).into(),
            Expr::Dict(_) => PythonType::Dict,
            Expr::DictComp(_) => PythonType::Dict,
            Expr::Set(_) => PythonType::Set,
            Expr::SetComp(_) => PythonType::Set,
            Expr::List(_) => PythonType::List,
            Expr::ListComp(_) => PythonType::List,
            Expr::Tuple(_) => PythonType::Tuple,
            Expr::GeneratorExp(_) => PythonType::Generator,
            Expr::FString(_) => PythonType::String,
            Expr::IfExp(ast::ExprIfExp { body, orelse, .. }) => {
                let body = PythonType::from(body.as_ref());
                let orelse = PythonType::from(orelse.as_ref());
                // TODO(charlie): If we have two known types, we should return a union. As-is,
                // callers that ignore the `Unknown` type will allow invalid expressions (e.g.,
                // if you're testing for strings, you may accept `String` or `Unknown`, and you'd
                // now accept, e.g., `1 if True else "a"`, which resolves to `Unknown`).
                if body == orelse {
                    body
                } else {
                    PythonType::Unknown
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                left, op, right, ..
            }) => {
                match op {
                    // Ex) "a" + "b"
                    Operator::Add => {
                        match (
                            PythonType::from(left.as_ref()),
                            PythonType::from(right.as_ref()),
                        ) {
                            (PythonType::String, PythonType::String) => return PythonType::String,
                            (PythonType::Bytes, PythonType::Bytes) => return PythonType::Bytes,
                            // TODO(charlie): If we have two known types, they may be incompatible.
                            // Return an error (e.g., for `1 + "a"`).
                            _ => {}
                        }
                    }
                    // Ex) "a" % "b"
                    Operator::Mod => match PythonType::from(left.as_ref()) {
                        PythonType::String => return PythonType::String,
                        PythonType::Bytes => return PythonType::Bytes,
                        _ => {}
                    },
                    _ => {}
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
            },
            _ => PythonType::Unknown,
        }
    }
}
