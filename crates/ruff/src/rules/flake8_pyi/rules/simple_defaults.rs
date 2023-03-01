use rustpython_parser::ast::{Arguments, Constant, Expr, ExprKind, Operator, Unaryop};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct TypedArgumentSimpleDefaults;
);
/// PYI011
impl Violation for TypedArgumentSimpleDefaults {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for typed arguments")
    }
}

define_violation!(
    pub struct ArgumentSimpleDefaults;
);
/// PYI014
impl Violation for ArgumentSimpleDefaults {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for arguments")
    }
}

const ALLOWED_ATTRIBUTES_IN_DEFAULTS: &[&[&str]] = &[
    &["sys", "stdin"],
    &["sys", "stdout"],
    &["sys", "stderr"],
    &["sys", "version"],
    &["sys", "version_info"],
    &["sys", "platform"],
    &["sys", "executable"],
    &["sys", "prefix"],
    &["sys", "exec_prefix"],
    &["sys", "base_prefix"],
    &["sys", "byteorder"],
    &["sys", "maxsize"],
    &["sys", "hexversion"],
    &["sys", "winver"],
];

fn is_valid_default_value_with_annotation(default: &Expr, checker: &Checker) -> bool {
    match &default.node {
        ExprKind::Constant {
            value: Constant::Ellipsis | Constant::None,
            ..
        } => {
            return true;
        }
        ExprKind::Constant {
            value: Constant::Str(..),
            ..
        } => return checker.locator.slice(&Range::from_located(default)).len() <= 50,
        ExprKind::Constant {
            value: Constant::Bytes(..),
            ..
        } => return checker.locator.slice(&Range::from_located(default)).len() <= 50,
        ExprKind::Constant {
            value: Constant::Int(..),
            ..
        } => {
            return checker.locator.slice(&Range::from_located(default)).len() <= 10;
        }
        ExprKind::UnaryOp {
            op: Unaryop::USub,
            operand,
        } => {
            if let ExprKind::Constant {
                value: Constant::Int(..),
                ..
            } = &operand.node
            {
                return checker.locator.slice(&Range::from_located(operand)).len() <= 10;
            }
        }
        ExprKind::BinOp {
            left,
            op: Operator::Add | Operator::Sub,
            right,
        } => {
            // 1 + 2j
            // 1 - 2j
            // -1 - 2j
            // -1 + 2j
            if let ExprKind::Constant {
                value: Constant::Complex { .. },
                ..
            } = right.node
            {
                // 1 + 2j
                // 1 - 2j
                if let ExprKind::Constant {
                    value: Constant::Int(..),
                    ..
                } = &left.node
                {
                    return checker.locator.slice(&Range::from_located(left)).len() <= 10;
                } else if let ExprKind::UnaryOp {
                    op: Unaryop::USub,
                    operand,
                } = &left.node
                {
                    // -1 + 2j
                    // -1 - 2j
                    if let ExprKind::Constant {
                        value: Constant::Int(..),
                        ..
                    } = &operand.node
                    {
                        return checker.locator.slice(&Range::from_located(operand)).len() <= 10;
                    }
                }
            }
        }
        // `sys.stdin`, etc.
        ExprKind::Attribute { .. } => {
            if checker
                .resolve_call_path(default)
                .map_or(false, |call_path| {
                    ALLOWED_ATTRIBUTES_IN_DEFAULTS
                        .iter()
                        .any(|target| call_path.as_slice() == *target)
                })
            {
                return true;
            }
        }
        _ => {}
    }
    false
}

/// PYI011
pub fn typed_argument_simple_defaults(checker: &mut Checker, args: &Arguments) {
    if !args.defaults.is_empty() {
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.defaults.get(i))
            {
                if arg.node.annotation.is_some() {
                    if !is_valid_default_value_with_annotation(default, checker) {
                        checker.diagnostics.push(Diagnostic::new(
                            TypedArgumentSimpleDefaults,
                            Range::from_located(default),
                        ));
                    }
                }
            }
        }
    }

    if !args.kw_defaults.is_empty() {
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                if kwarg.node.annotation.is_some() {
                    if !is_valid_default_value_with_annotation(default, checker) {
                        checker.diagnostics.push(Diagnostic::new(
                            TypedArgumentSimpleDefaults,
                            Range::from_located(default),
                        ));
                    }
                }
            }
        }
    }
}

/// PYI014
pub fn argument_simple_defaults(checker: &mut Checker, args: &Arguments) {
    if !args.defaults.is_empty() {
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.defaults.get(i))
            {
                if arg.node.annotation.is_none() {
                    if !is_valid_default_value_with_annotation(default, checker) {
                        checker.diagnostics.push(Diagnostic::new(
                            ArgumentSimpleDefaults,
                            Range::from_located(default),
                        ));
                    }
                }
            }
        }
    }

    if !args.kw_defaults.is_empty() {
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                if kwarg.node.annotation.is_none() {
                    if !is_valid_default_value_with_annotation(default, checker) {
                        checker.diagnostics.push(Diagnostic::new(
                            ArgumentSimpleDefaults,
                            Range::from_located(default),
                        ));
                    }
                }
            }
        }
    }
}
