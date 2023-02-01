use ruff_macros::derive_message_formats;
use rustpython_ast::{Arguments, ExprKind};
use rustpython_parser::ast::{Constant, Expr};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::violation::Violation;

define_violation!(
    pub struct BooleanPositionalArgInFunctionDefinition;
);
impl Violation for BooleanPositionalArgInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional arg in function definition")
    }
}

define_violation!(
    pub struct BooleanDefaultValueInFunctionDefinition;
);
impl Violation for BooleanDefaultValueInFunctionDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean default value in function definition")
    }
}

define_violation!(
    pub struct BooleanPositionalValueInFunctionCall;
);
impl Violation for BooleanPositionalValueInFunctionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional value in function call")
    }
}

const FUNC_NAME_ALLOWLIST: &[&str] = &[
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
];

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
fn allow_boolean_trap(func: &Expr) -> bool {
    if let ExprKind::Attribute { attr, .. } = &func.node {
        return FUNC_NAME_ALLOWLIST.contains(&attr.as_ref());
    }

    if let ExprKind::Name { id, .. } = &func.node {
        return FUNC_NAME_ALLOWLIST.contains(&id.as_ref());
    }

    false
}

fn is_boolean_arg(arg: &Expr) -> bool {
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
            .push(Diagnostic::new(kind, Range::from_located(arg)));
    }
}

pub fn check_positional_boolean_in_def(checker: &mut Checker, arguments: &Arguments) {
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
            Range::from_located(arg),
        ));
    }
}

pub fn check_boolean_default_value_in_function_definition(
    checker: &mut Checker,
    arguments: &Arguments,
) {
    for arg in &arguments.defaults {
        add_if_boolean(checker, arg, BooleanDefaultValueInFunctionDefinition.into());
    }
}

pub fn check_boolean_positional_value_in_function_call(
    checker: &mut Checker,
    args: &[Expr],
    func: &Expr,
) {
    for arg in args {
        if allow_boolean_trap(func) {
            continue;
        }
        add_if_boolean(checker, arg, BooleanPositionalValueInFunctionCall.into());
    }
}
