use rustpython_parser::ast::{Constant, Expr, ExprKind, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct FStringInI18NFuncCall;

impl Violation for FStringInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

#[violation]
pub struct FormatInI18NFuncCall;

impl Violation for FormatInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`format` method argument is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}
#[violation]
pub struct PrintfInI18NFuncCall;

impl Violation for PrintfInI18NFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("printf-style format is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// Returns true if the [`Expr`] is an internationalization function call.
pub fn is_i18n_func_call(func: &Expr, functions_names: &[String]) -> bool {
    if let ExprKind::Name { id, .. } = &func.node {
        functions_names.contains(id)
    } else {
        false
    }
}

/// INT001
pub fn f_string_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if matches!(first.node, ExprKind::JoinedStr { .. }) {
            return Some(Diagnostic::new(
                FStringInI18NFuncCall {},
                Range::from(first),
            ));
        }
    }
    None
}

/// INT002
pub fn format_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::Call { func, .. } = &first.node {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "format" {
                    return Some(Diagnostic::new(FormatInI18NFuncCall {}, Range::from(first)));
                }
            }
        }
    }
    None
}

/// INT003
pub fn printf_in_i18n_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::BinOp {
            op: Operator::Mod { .. },
            left,
            ..
        } = &first.node
        {
            if let ExprKind::Constant {
                value: Constant::Str(_),
                ..
            } = left.node
            {
                return Some(Diagnostic::new(PrintfInI18NFuncCall {}, Range::from(first)));
            }
        }
    }
    None
}
