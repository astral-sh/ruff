use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::type_param_name;

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
#[derive(ViolationMetadata)]
pub(crate) struct TypeBivariance {
    kind: VarKind,
    param_name: Option<String>,
}

impl Violation for TypeBivariance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeBivariance { kind, param_name } = self;
        match param_name {
            None => format!("`{kind}` cannot be both covariant and contravariant"),
            Some(param_name) => {
                format!("`{kind}` \"{param_name}\" cannot be both covariant and contravariant",)
            }
        }
    }
}

/// PLC0131
pub(crate) fn type_bivariance(checker: &Checker, value: &Expr) {
    // If the typing modules were never imported, we'll never match below.
    if !checker.semantic().seen_typing() {
        return;
    }

    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = value
    else {
        return;
    };

    let Some(covariant) = arguments
        .find_keyword("covariant")
        .map(|keyword| &keyword.value)
    else {
        return;
    };

    let Some(contravariant) = arguments
        .find_keyword("contravariant")
        .map(|keyword| &keyword.value)
    else {
        return;
    };

    if is_const_true(covariant) && is_const_true(contravariant) {
        let Some(kind) =
            checker
                .semantic()
                .resolve_qualified_name(func)
                .and_then(|qualified_name| {
                    if checker
                        .semantic()
                        .match_typing_qualified_name(&qualified_name, "ParamSpec")
                    {
                        Some(VarKind::ParamSpec)
                    } else if checker
                        .semantic()
                        .match_typing_qualified_name(&qualified_name, "TypeVar")
                    {
                        Some(VarKind::TypeVar)
                    } else {
                        None
                    }
                })
        else {
            return;
        };

        checker.report_diagnostic(Diagnostic::new(
            TypeBivariance {
                kind,
                param_name: type_param_name(arguments).map(ToString::to_string),
            },
            func.range(),
        ));
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
