use rustpython_parser::ast::{self, Arguments, Constant, Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;

use crate::checkers::ast::Checker;

#[violation]
pub struct BooleanPositionalArgInFunctionDefinition;

impl Violation for BooleanPositionalArgInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional arg in function definition")
    }
}

#[violation]
pub struct BooleanDefaultValueInFunctionDefinition;

impl Violation for BooleanDefaultValueInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean default value in function definition")
    }
}

#[violation]
pub struct BooleanPositionalValueInFunctionCall;

impl Violation for BooleanPositionalValueInFunctionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional value in function call")
    }
}

const FUNC_CALL_NAME_ALLOWLIST: &[&str] = &[
    "append",
    "assertEqual",
    "assertEquals",
    "assertNotEqual",
    "assertNotEquals",
    "bytes",
    "count",
    "failIfEqual",
    "failUnlessEqual",
    "float",
    "fromkeys",
    "get",
    "getattr",
    "getboolean",
    "getfloat",
    "getint",
    "index",
    "insert",
    "int",
    "param",
    "pop",
    "remove",
    "setattr",
    "setdefault",
    "str",
];

const FUNC_DEF_NAME_ALLOWLIST: &[&str] = &["__setitem__"];

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
fn allow_boolean_trap(func: &Expr) -> bool {
    if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = &func {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&attr.as_ref());
    }

    if let Expr::Name(ast::ExprName { id, .. }) = &func {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&id.as_ref());
    }

    false
}

const fn is_boolean_arg(arg: &Expr) -> bool {
    matches!(
        &arg,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(_),
            ..
        })
    )
}

fn add_if_boolean(checker: &mut Checker, arg: &Expr, kind: DiagnosticKind) {
    if is_boolean_arg(arg) {
        checker.diagnostics.push(Diagnostic::new(kind, arg.range()));
    }
}

pub(crate) fn check_positional_boolean_in_def(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Expr],
    arguments: &Arguments,
) {
    if FUNC_DEF_NAME_ALLOWLIST.contains(&name) {
        return;
    }

    if decorator_list.iter().any(|expr| {
        collect_call_path(expr).map_or(false, |call_path| call_path.as_slice() == [name, "setter"])
    }) {
        return;
    }

    for arg in arguments.posonlyargs.iter().chain(arguments.args.iter()) {
        if arg.annotation.is_none() {
            continue;
        }
        let Some(expr) = &arg.annotation else {
            continue;
        };

        // check for both bool (python class) and 'bool' (string annotation)
        let hint = match expr.as_ref() {
            Expr::Name(name) => &name.id == "bool",
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                ..
            }) => value == "bool",
            _ => false,
        };
        if !hint {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            BooleanPositionalArgInFunctionDefinition,
            arg.range(),
        ));
    }
}

pub(crate) fn check_boolean_default_value_in_function_definition(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Expr],
    arguments: &Arguments,
) {
    if FUNC_DEF_NAME_ALLOWLIST.contains(&name) {
        return;
    }

    if decorator_list.iter().any(|expr| {
        collect_call_path(expr).map_or(false, |call_path| call_path.as_slice() == [name, "setter"])
    }) {
        return;
    }

    for arg in &arguments.defaults {
        add_if_boolean(checker, arg, BooleanDefaultValueInFunctionDefinition.into());
    }
}

pub(crate) fn check_boolean_positional_value_in_function_call(
    checker: &mut Checker,
    args: &[Expr],
    func: &Expr,
) {
    if allow_boolean_trap(func) {
        return;
    }
    for arg in args {
        add_if_boolean(checker, arg, BooleanPositionalValueInFunctionCall.into());
    }
}
