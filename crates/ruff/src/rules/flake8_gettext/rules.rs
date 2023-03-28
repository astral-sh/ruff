use rustpython_parser::ast::{Constant, Expr, ExprKind, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct FStringInGetTextFuncCall;

impl Violation for FStringInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

#[violation]
pub struct FormatInGetTextFuncCall;

impl Violation for FormatInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`format` method argument is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}
#[violation]
pub struct PrintfInGetTextFuncCall;

impl Violation for PrintfInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("printf-style format is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// Returns true if the [`Expr`] is an internationalization function call.
pub fn is_gettext_func_call(func: &Expr, functions_names: &[String]) -> bool {
    if let ExprKind::Name { id, .. } = &func.node {
        functions_names.contains(id)
    } else {
        false
    }
}

/// INT001
pub fn f_string_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if matches!(first.node, ExprKind::JoinedStr { .. }) {
            return Some(Diagnostic::new(
                FStringInGetTextFuncCall {},
                Range::from(first),
            ));
        }
    }
    None
}

/// INT002
pub fn format_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let ExprKind::Call { func, .. } = &first.node {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "format" {
                    return Some(Diagnostic::new(
                        FormatInGetTextFuncCall {},
                        Range::from(first),
                    ));
                }
            }
        }
    }
    None
}

/// INT003
pub fn printf_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
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
                return Some(Diagnostic::new(
                    PrintfInGetTextFuncCall {},
                    Range::from(first),
                ));
            }
        }
    }
    None
}
