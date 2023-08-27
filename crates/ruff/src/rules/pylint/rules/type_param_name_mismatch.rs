use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::type_param_name;

/// ## What it does
/// Checks for `TypeVar`, `TypeVarTuple`, `ParamSpec`, and `NewType`
/// definitions in which the name of the type parameter does not match the name
/// of the variable to which it is assigned.
///
/// ## Why is this bad?
/// When defining a `TypeVar` or a related type parameter, Python allows you to
/// provide a name for the type parameter. According to [PEP 484], the name
/// provided to the `TypeVar` constructor must be equal to the name of the
/// variable to which it is assigned.
///
/// ## Example
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("U")
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T")
/// ```
///
/// ## References
/// - [Python documentation: `typing` — Support for type hints](https://docs.python.org/3/library/typing.html)
/// - [PEP 484 – Type Hints: Generics](https://peps.python.org/pep-0484/#generics)
///
/// [PEP 484]:https://peps.python.org/pep-0484/#generics
#[violation]
pub struct TypeParamNameMismatch {
    kind: VarKind,
    var_name: String,
    param_name: String,
}

impl Violation for TypeParamNameMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeParamNameMismatch {
            kind,
            var_name,
            param_name,
        } = self;
        format!("`{kind}` name `{param_name}` does not match assigned variable name `{var_name}`")
    }
}

/// PLC0132
pub(crate) fn type_param_name_mismatch(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };

    let Expr::Name(ast::ExprName { id: var_name, .. }) = &target else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = value
    else {
        return;
    };

    let Some(param_name) = type_param_name(arguments) else {
        return;
    };

    if var_name == param_name {
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
            } else if checker
                .semantic()
                .match_typing_call_path(&call_path, "TypeVarTuple")
            {
                Some(VarKind::TypeVarTuple)
            } else if checker
                .semantic()
                .match_typing_call_path(&call_path, "NewType")
            {
                Some(VarKind::NewType)
            } else {
                None
            }
        })
    else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        TypeParamNameMismatch {
            kind,
            var_name: var_name.to_string(),
            param_name: param_name.to_string(),
        },
        value.range(),
    ));
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum VarKind {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
    NewType,
}

impl fmt::Display for VarKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarKind::TypeVar => fmt.write_str("TypeVar"),
            VarKind::ParamSpec => fmt.write_str("ParamSpec"),
            VarKind::TypeVarTuple => fmt.write_str("TypeVarTuple"),
            VarKind::NewType => fmt.write_str("NewType"),
        }
    }
}
