use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Arguments, Constant, ExprKind, Located, Operator, Unaryop};
use std::collections::HashSet;

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

fn is_valid_default_value_with_annotation(default: &Located<ExprKind>) -> bool {
    match &default.node {
        ExprKind::Constant {
            value: Constant::Ellipsis | Constant::None,
            ..
        } => {
            return true;
        }
        ExprKind::Constant {
            value: Constant::Str(str),
            ..
        } => return str.len() <= 50,
        ExprKind::Constant {
            value: Constant::Bytes(bytes),
            ..
        } => return bytes.len() <= 50,
        ExprKind::Constant {
            value: Constant::Int(int),
            ..
        } => {
            return int.to_string().len() <= 50;
        }
        ExprKind::UnaryOp {
            op: Unaryop::USub,
            operand,
        } => {
            if let ExprKind::Constant {
                value: Constant::Int(int),
                ..
            } = &operand.node
            {
                return int.to_string().len() <= 10;
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
                    value: Constant::Int(int),
                    ..
                } = &left.node
                {
                    return int.to_string().len() <= 10;
                }

                if let ExprKind::UnaryOp {
                    op: Unaryop::USub,
                    operand,
                } = &left.node
                {
                    // -1 + 2j
                    // -1 - 2j
                    if let ExprKind::Constant {
                        value: Constant::Int(intcontent),
                        ..
                    } = &operand.node
                    {
                        return intcontent.to_string().len() <= 10;
                    }
                }
            }
        }
        // sys.stdin
        ExprKind::Attribute { value, attr, .. } => {
            if let ExprKind::Name { id, .. } = &value.node {
                let allowed_attr_defaults = HashSet::from([
                    "sys.base_prefix",
                    "sys.byteorder",
                    "sys.exec_prefix",
                    "sys.executable",
                    "sys.hexversion",
                    "sys.maxsize",
                    "sys.platform",
                    "sys.prefix",
                    "sys.stdin",
                    "sys.stdout",
                    "sys.stderr",
                    "sys.version",
                    "sys.version_info",
                    "sys.winver",
                ]);
                return allowed_attr_defaults.contains(format!("{id}.{attr}").as_str());
            }
        }
        _ => return false,
    }
    false
}

/// PYI011
pub fn typed_argument_simple_defaults(checker: &mut Checker, expr: &Arguments) {
    for (default, arg) in expr.defaults.iter().zip(&expr.args) {
        if arg.node.annotation.is_some() {
            if !is_valid_default_value_with_annotation(default) {
                checker.diagnostics.push(Diagnostic::new(
                    TypedArgumentSimpleDefaults,
                    Range::from_located(default),
                ));
            }
        }
    }
}

/// PYI014
pub fn argument_simple_defaults(checker: &mut Checker, expr: &Arguments) {
    for (default, arg) in expr.defaults.iter().zip(&expr.args) {
        if arg.node.annotation.is_none() {
            if !is_valid_default_value_with_annotation(default) {
                checker.diagnostics.push(Diagnostic::new(
                    ArgumentSimpleDefaults,
                    Range::from_located(default),
                ));
            }
        }
    }
}
