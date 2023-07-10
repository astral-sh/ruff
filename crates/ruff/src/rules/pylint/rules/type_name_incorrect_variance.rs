use std::fmt;

use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::type_param_name;

/// ## What it does
/// Checks for type names that do not match the variance of their associated
/// type parameter.
///
/// ## Why is this bad?
/// [PEP 484] recommends the use of the `_co` and `_contra` suffixes for
/// covariant and contravariant type parameters, respectively (while invariant
/// type parameters should not have any such suffix).
///
/// ## Example
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T", covariant=True)
/// U = TypeVar("U", contravariant=True)
/// V_co = TypeVar("V_co")
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypeVar
///
/// T_co = TypeVar("T_co", covariant=True)
/// U_contra = TypeVar("U_contra", contravariant=True)
/// V = TypeVar("V")
/// ```
///
/// ## References
/// - [Python documentation: `typing` — Support for type hints](https://docs.python.org/3/library/typing.html)
/// - [PEP 483 – The Theory of Type Hints: Covariance and Contravariance](https://peps.python.org/pep-0483/#covariance-and-contravariance)
/// - [PEP 484 – Type Hints: Covariance and contravariance](https://peps.python.org/pep-0484/#covariance-and-contravariance)
///
/// [PEP 484]: https://www.python.org/dev/peps/pep-0484/
#[violation]
pub struct TypeNameIncorrectVariance {
    kind: VarKind,
    param_name: String,
}

impl Violation for TypeNameIncorrectVariance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeNameIncorrectVariance { kind, param_name } = self;
        format!("`{kind}` name \"{param_name}\" does not match variance")
    }
}

/// PLC0105
pub(crate) fn type_name_incorrect_variance(checker: &mut Checker, value: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        ..
    }) = value
    else {
        return;
    };

    let Some(param_name) = type_param_name(args, keywords) else {
        return;
    };

    let covariant = keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "covariant")
        })
        .map(|keyword| &keyword.value);

    let contravariant = keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "contravariant")
        })
        .map(|keyword| &keyword.value);

    if !mismatch(param_name, covariant, contravariant) {
        return;
    }

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

    checker.diagnostics.push(Diagnostic::new(
        TypeNameIncorrectVariance {
            kind,
            param_name: param_name.to_string(),
        },
        func.range(),
    ));
}

/// Returns `true` if the parameter name does not match its type variance.
fn mismatch(param_name: &str, covariant: Option<&Expr>, contravariant: Option<&Expr>) -> bool {
    if param_name.ends_with("_co") {
        covariant.map_or(true, |covariant| !is_const_true(covariant))
    } else if param_name.ends_with("_contra") {
        contravariant.map_or(true, |contravariant| !is_const_true(contravariant))
    } else {
        covariant.map_or(false, is_const_true)
            || contravariant.map_or(false, is_const_true)
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
