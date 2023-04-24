use rustpython_parser::ast::{Arguments, Constant, Expr, ExprKind};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::types::Range;

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
    "assertEqual",
    "assertEquals",
    "assertNotEqual",
    "assertNotEquals",
    "failIfEqual",
    "failUnlessEqual",
    "fromkeys",
    "get",
    "getattr",
    "index",
    "pop",
    "setattr",
    "setdefault",
    "str",
    "bytes",
    "int",
    "float",
    "getint",
    "getfloat",
    "getboolean",
];

const FUNC_DEF_NAME_ALLOWLIST: &[&str] = &["__setitem__"];

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
fn allow_boolean_trap(func: &Expr) -> bool {
    if let ExprKind::Attribute { attr, .. } = &func.node {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&attr.as_ref());
    }

    if let ExprKind::Name { id, .. } = &func.node {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&id.as_ref());
    }

    false
}

const fn is_boolean_arg(arg: &Expr) -> bool {
    matches!(
        &arg.node,
        ExprKind::Constant {
            value: Constant::Bool(_),
            ..
        }
    )
}

fn add_if_boolean(checker: &mut Checker, arg: &Expr, kind: DiagnosticKind) {
    if is_boolean_arg(arg) {
        checker
            .diagnostics
            .push(Diagnostic::new(kind, Range::from(arg)));
    }
}

pub fn check_positional_boolean_in_def(
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
        if arg.node.annotation.is_none() {
            continue;
        }
        let Some(expr) = &arg.node.annotation else {
            continue;
        };

        // check for both bool (python class) and 'bool' (string annotation)
        let hint = match &expr.node {
            ExprKind::Name { id, .. } => id == "bool",
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } => value == "bool",
            _ => false,
        };
        if !hint {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            BooleanPositionalArgInFunctionDefinition,
            Range::from(arg),
        ));
    }
}

pub fn check_boolean_default_value_in_function_definition(
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

pub fn check_boolean_positional_value_in_function_call(
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
