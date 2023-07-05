use std::fmt;

use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `TypeVar` and `ParamSpec` definitions in which the type is
/// both covariant and contravariant.
///
/// ## Why is this bad?
/// By default, Python's generic types are invariant, but can be marked as
/// either covariant or contravariant via the `covariant` and `contravariant`
/// keyword arguments. While the API does allow you to mark a type as both
/// covariant and contravariant, this is not supported by the type system,
/// and should be avoided.
///
/// Instead, change the variance of the type to be either covariant,
/// contravariant, or invariant. If you want to describe both covariance and
/// contravariance, consider using two separate type parameters.
///
/// For context: an "invariant" generic type only accepts values that exactly
/// match the type parameter; for example, `list[Dog]` accepts only `list[Dog]`,
/// not `list[Animal]` (superclass) or `list[Bulldog]` (subclass). This is
/// the default behavior for Python's generic types.
///
/// A "covariant" generic type accepts subclasses of the type parameter; for
/// example, `Sequence[Animal]` accepts `Sequence[Dog]`. A "contravariant"
/// generic type accepts superclasses of the type parameter; for example,
/// `Callable[Dog]` accepts `Callable[Animal]`.
///
/// ## Example
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T", covariant=True, contravariant=True)
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

impl Violation for TypeBivariance {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Type parameter cannot be both covariant and contravariant")
    }
}

/// PLC0131
pub(crate) fn type_bivariance(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };

    let Expr::Call(ast::ExprCall { func, keywords, .. }) = value else {
        return;
    };

    let Some(covariant) = keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "covariant")
        })
        .map(|keyword| &keyword.value)
    else {
        return;
    };

    let Some(contravariant) = keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "contravariant")
        })
        .map(|keyword| &keyword.value)
    else {
        return;
    };

    if is_const_true(covariant) && is_const_true(contravariant) {
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
