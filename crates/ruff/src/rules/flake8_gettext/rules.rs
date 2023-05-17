use rustpython_parser::ast::{self, Constant, Expr, Operator, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
pub(crate) fn is_gettext_func_call(func: &Expr, functions_names: &[String]) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        functions_names.contains(id.as_ref())
    } else {
        false
    }
}

/// INT001
pub(crate) fn f_string_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if first.is_joined_str_expr() {
            return Some(Diagnostic::new(FStringInGetTextFuncCall {}, first.range()));
        }
    }
    None
}

/// INT002
pub(crate) fn format_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let Expr::Call(ast::ExprCall { func, .. }) = &first {
            if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() {
                if attr == "format" {
                    return Some(Diagnostic::new(FormatInGetTextFuncCall {}, first.range()));
                }
            }
        }
    }
    None
}

/// INT003
pub(crate) fn printf_in_gettext_func_call(args: &[Expr]) -> Option<Diagnostic> {
    if let Some(first) = args.first() {
        if let Expr::BinOp(ast::ExprBinOp {
            op: Operator::Mod { .. },
            left,
            ..
        }) = &first
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            }) = left.as_ref()
            {
                return Some(Diagnostic::new(PrintfInGetTextFuncCall {}, first.range()));
            }
        }
    }
    None
}
