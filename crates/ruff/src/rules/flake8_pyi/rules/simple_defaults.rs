use rustpython_parser::ast::{Arguments, Constant, Expr, ExprKind, Operator, Unaryop};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct TypedArgumentDefaultInStub;

/// PYI011
impl AlwaysAutofixableViolation for TypedArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for typed arguments")
    }

    fn autofix_title(&self) -> String {
        "Replace default value by `...`".to_string()
    }
}

#[violation]
pub struct ArgumentDefaultInStub;

/// PYI014
impl Violation for ArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for arguments")
    }
}

const ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS: &[&[&str]] = &[
    &["math", "inf"],
    &["math", "nan"],
    &["math", "e"],
    &["math", "pi"],
    &["math", "tau"],
];

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
        } => return checker.locator.slice(default).len() <= 50,
        ExprKind::Constant {
            value: Constant::Bytes(..),
            ..
        } => return checker.locator.slice(default).len() <= 50,
        // Ex) `123`, `True`, `False`, `3.14`
        ExprKind::Constant {
            value: Constant::Int(..) | Constant::Bool(..) | Constant::Float(..),
            ..
        } => {
            return checker.locator.slice(default).len() <= 10;
        }
        // Ex) `2j`
        ExprKind::Constant {
            value: Constant::Complex { real, .. },
            ..
        } => {
            if *real == 0.0 {
                return checker.locator.slice(default).len() <= 10;
            }
        }
        ExprKind::UnaryOp {
            op: Unaryop::USub,
            operand,
        } => {
            // Ex) `-1`, `-3.14`
            if let ExprKind::Constant {
                value: Constant::Int(..) | Constant::Float(..),
                ..
            } = &operand.node
            {
                return checker.locator.slice(operand).len() <= 10;
            }
            // Ex) `-2j`
            if let ExprKind::Constant {
                value: Constant::Complex { real, .. },
                ..
            } = &operand.node
            {
                if *real == 0.0 {
                    return checker.locator.slice(operand).len() <= 10;
                }
            }
            // Ex) `-math.inf`, `-math.pi`, etc.
            if let ExprKind::Attribute { .. } = &operand.node {
                if checker
                    .ctx
                    .resolve_call_path(operand)
                    .map_or(false, |call_path| {
                        ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS.iter().any(|target| {
                            // reject `-math.nan`
                            call_path.as_slice() == *target && *target != ["math", "nan"]
                        })
                    })
                {
                    return true;
                }
            }
        }
        ExprKind::BinOp {
            left,
            op: Operator::Add | Operator::Sub,
            right,
        } => {
            // Ex) `1 + 2j`, `1 - 2j`, `-1 - 2j`, `-1 + 2j`
            if let ExprKind::Constant {
                value: Constant::Complex { .. },
                ..
            } = right.node
            {
                // Ex) `1 + 2j`, `1 - 2j`
                if let ExprKind::Constant {
                    value: Constant::Int(..) | Constant::Float(..),
                    ..
                } = &left.node
                {
                    return checker.locator.slice(left).len() <= 10;
                } else if let ExprKind::UnaryOp {
                    op: Unaryop::USub,
                    operand,
                } = &left.node
                {
                    // Ex) `-1 + 2j`, `-1 - 2j`
                    if let ExprKind::Constant {
                        value: Constant::Int(..) | Constant::Float(..),
                        ..
                    } = &operand.node
                    {
                        return checker.locator.slice(operand).len() <= 10;
                    }
                }
            }
        }
        // Ex) `math.inf`, `sys.stdin`, etc.
        ExprKind::Attribute { .. } => {
            if checker
                .ctx
                .resolve_call_path(default)
                .map_or(false, |call_path| {
                    ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS
                        .iter()
                        .chain(ALLOWED_ATTRIBUTES_IN_DEFAULTS.iter())
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
                        let mut diagnostic =
                            Diagnostic::new(TypedArgumentDefaultInStub, Range::from(default));

                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::replacement(
                                "...".to_string(),
                                default.location,
                                default.end_location.unwrap(),
                            ));
                        }

                        checker.diagnostics.push(diagnostic);
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
                        let mut diagnostic =
                            Diagnostic::new(TypedArgumentDefaultInStub, Range::from(default));

                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::replacement(
                                "...".to_string(),
                                default.location,
                                default.end_location.unwrap(),
                            ));
                        }

                        checker.diagnostics.push(diagnostic);
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
                        checker
                            .diagnostics
                            .push(Diagnostic::new(ArgumentDefaultInStub, Range::from(default)));
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
                        checker
                            .diagnostics
                            .push(Diagnostic::new(ArgumentDefaultInStub, Range::from(default)));
                    }
                }
            }
        }
    }
}
