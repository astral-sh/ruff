use rustpython_parser::ast::{Constant, Expr, ExprKind, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct FStringInI18NFuncCall {}

impl Violation for FStringInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string is resolved before function call, consider using something like `_(\"string %s\") % arg`")
    }
}

#[violation]
pub struct FormatInI18NFuncCall {}

impl Violation for FormatInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`format` method argument is resolved before function call, consider using something like `_(\"string %s\") % arg`")
    }
}
#[violation]
pub struct PrintFInI18NFuncCall {}

impl Violation for PrintFInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("printf-style format is resolved before function call, consider using something like `_(\"string %s\") % arg`")
    }
}

pub fn function_needs_check(func: &Expr, functions_names: &[String]) -> bool {
    if let ExprKind::Name { id, ctx: _ } = &func.node {
        return functions_names.iter().any(|x| x == id);
    }
    false
}

pub fn f_string_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::JoinedStr { .. } = first.node {
            return Some(Diagnostic::new(
                FStringInI18NFuncCall {},
                Range::from(first),
            ));
        }
    }
    None
}

pub fn format_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::Call { func, .. } = &first.node {
            return match func.node {
                ExprKind::Attribute { ref attr, .. } if attr == "format" => {
                    Some(Diagnostic::new(FormatInI18NFuncCall {}, Range::from(first)))
                }
                _ => None,
            };
        }
    }
    None
}

pub fn printf_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::BinOp {
            op: Operator::Mod { .. },
            left,
            ..
        } = &first.node
        {
            return match left.node {
                ExprKind::Constant {
                    value: Constant::Str(_),
                    ..
                } => Some(Diagnostic::new(PrintFInI18NFuncCall {}, Range::from(first))),
                _ => None,
            };
        }
    }
    None
}
