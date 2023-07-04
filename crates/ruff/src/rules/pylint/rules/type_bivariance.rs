use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use rustpython_parser::ast::{self, Expr, Ranged};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `TypeVar` and `ParamSpec` definitions in which the type variance
/// is both covariant and contravariant.
///
/// ## Why is this bad?
/// By default, generic types are invariant. This means that a `list[Animal]`
/// accepts `Animal` elements and nothing else (not even `Animal` subclasses
/// or superclasses).
///
/// Sometimes you want to allow a generic type to be covariant or
/// contravariant. Covariance means that a generic type accepts subclasses of
/// the type parameter; for example, `list[Animal]` accepts `list[Dog]`. On the
/// other hand, contravariance means that a generic type accepts superclasses
/// of the type parameter; for example, `list[Dog]` accepts `list[Animal]`.
///
/// However, a type cannot be both covariant and contravariant. This is because
/// a type cannot be both a subclass and a superclass of another type.
///
/// Instead, change the variance of the type to be either covariant,
/// contravariant, or invariant. If you want to describe both covariance and
/// contravariance, consider using two type parameters, one for covariance and
/// one for contravariance.
///
/// ## Example
/// ```python
/// from typing import TypeVar
///
/// T_bi = TypeVar("T_bi", covariant=True, contravariant=True)
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypeVar
///
/// T_co = TypeVar("T_co", covariant=True)
/// T_contra = TypeVar("T_contra", contravariant=True)
/// ```
///
/// ## References
/// - [Python documentation: `typing` — Support for type hints](https://docs.python.org/3/library/typing.html)
/// - [PEP 483 – The Theory of Type Hints: Covariance and Contravariance](https://peps.python.org/pep-0483/#covariance-and-contravariance)
/// - [PEP 484 – Type Hints: Covariance and contravariance](https://peps.python.org/pep-0484/#covariance-and-contravariance)
#[violation]
pub struct TypeBivariance {
    kind: VarKind,
}

/// PLC0131
pub(crate) fn type_bivariance(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };
    let Expr::Call(call) = value else { return };
    let covariant = call
        .keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "covariant")
        })
        .map(|keyword| &keyword.value);
    let contravariant = call
        .keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "contravariant")
        })
        .map(|keyword| &keyword.value);
    if let (Some(covariant), Some(contravariant)) = (covariant, contravariant) {
        if is_const_true(covariant) && is_const_true(contravariant) {
            let Expr::Call(ast::ExprCall { func, .. }) = value else {
                return;
            };
            let Some(kind) = checker
                .semantic()
                .resolve_call_path(func)
                .and_then(|call_path| {
                    if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "ParamSpec")
                    {
                        Some(VarKind::ParamSpec)
                    } else if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "TypeVar")
                    {
                        Some(VarKind::TypeVar)
                    } else {
                        None
                    }
                })
                else {
                    return;
                };
            checker
                .diagnostics
                .push(Diagnostic::new(TypeBivariance { kind }, target.range()));
        }
    }
}

impl Violation for TypeBivariance {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Type variance cannot be both covariant and contravariant")
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum VarKind {
    TypeVar,
    ParamSpec,
}

impl fmt::Display for VarKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarKind::TypeVar => fmt.write_str("TypeVar"),
            VarKind::ParamSpec => fmt.write_str("ParamSpec"),
        }
    }
}
