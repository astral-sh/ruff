use rustpython_ast::{Arguments, ExprKind};
use rustpython_parser::ast::{Constant, Expr};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn is_boolean_arg(arg: &Expr) -> bool {
    matches!(
        &arg.node,
        ExprKind::Constant {
            value: Constant::Bool(_),
            ..
        }
    )
}

fn add_if_boolean(checker: &mut Checker, arg: &Expr, kind: CheckKind) {
    if is_boolean_arg(arg) {
        checker.add_check(Check::new(kind, Range::from_located(arg)));
    }
}

pub fn check_positional_boolean_in_def(checker: &mut Checker, arguments: &Arguments) {
    for arg in arguments.posonlyargs.iter().chain(arguments.args.iter()) {
        if arg.node.annotation.is_none() {
            continue;
        }

        if let Some(expr) = &arg.node.annotation {
            // check for both bool (python class) and 'bool' (string annotation)
            let hint = match &expr.node {
                ExprKind::Name { id, .. } => id == "bool",
                ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } => value == "bool",
                _ => false,
            };
            if hint {
                checker.add_check(Check::new(
                    CheckKind::BooleanPositionalArgInFunctionDefinition,
                    Range::from_located(arg),
                ));
            }
        }
    }
}

pub fn check_boolean_default_value_in_function_definition(
    checker: &mut Checker,
    arguments: &Arguments,
) {
    for arg in arguments.defaults.iter() {
        add_if_boolean(
            checker,
            arg,
            CheckKind::BooleanDefaultValueInFunctionDefinition,
        );
    }
}

pub fn check_boolean_positional_value_in_function_call(checker: &mut Checker, args: &[Expr]) {
    for arg in args {
        add_if_boolean(
            checker,
            arg,
            CheckKind::BooleanPositionalValueInFunctionCall,
        );
    }
}
