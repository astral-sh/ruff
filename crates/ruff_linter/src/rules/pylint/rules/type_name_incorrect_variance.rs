use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

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
    variance: VarVariance,
    replacement_name: String,
}

impl Violation for TypeNameIncorrectVariance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeNameIncorrectVariance {
            kind,
            param_name,
            variance,
            replacement_name,
        } = self;
        format!("`{kind}` name \"{param_name}\" does not reflect its {variance}; consider renaming it to \"{replacement_name}\"")
    }
}

/// PLC0105
pub(crate) fn type_name_incorrect_variance(checker: &mut Checker, value: &Expr) {
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

    let Some(param_name) = type_param_name(arguments) else {
        return;
    };

    let covariant = arguments
        .find_keyword("covariant")
        .map(|keyword| &keyword.value);

    let contravariant = arguments
        .find_keyword("contravariant")
        .map(|keyword| &keyword.value);

    if !mismatch(param_name, covariant, contravariant) {
        return;
    }

    let Some(kind) = checker
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

    let variance = variance(covariant, contravariant);
    let name_root = param_name
        .trim_end_matches("_co")
        .trim_end_matches("_contra");
    let replacement_name: String = match variance {
        VarVariance::Bivariance => return, // Bivariate types are invalid, so ignore them for this rule.
        VarVariance::Covariance => format!("{name_root}_co"),
        VarVariance::Contravariance => format!("{name_root}_contra"),
        VarVariance::Invariance => name_root.to_string(),
    };

    checker.diagnostics.push(Diagnostic::new(
        TypeNameIncorrectVariance {
            kind,
            param_name: param_name.to_string(),
            variance,
            replacement_name,
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
        covariant.is_some_and(is_const_true) || contravariant.is_some_and(is_const_true)
    }
}

/// Return the variance of the type parameter.
fn variance(covariant: Option<&Expr>, contravariant: Option<&Expr>) -> VarVariance {
    match (
        covariant.map(is_const_true),
        contravariant.map(is_const_true),
    ) {
        (Some(true), Some(true)) => VarVariance::Bivariance,
        (Some(true), _) => VarVariance::Covariance,
        (_, Some(true)) => VarVariance::Contravariance,
        _ => VarVariance::Invariance,
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum VarVariance {
    Bivariance,
    Covariance,
    Contravariance,
    Invariance,
}

impl fmt::Display for VarVariance {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarVariance::Bivariance => fmt.write_str("bivariance"),
            VarVariance::Covariance => fmt.write_str("covariance"),
            VarVariance::Contravariance => fmt.write_str("contravariance"),
            VarVariance::Invariance => fmt.write_str("invariance"),
        }
    }
}
