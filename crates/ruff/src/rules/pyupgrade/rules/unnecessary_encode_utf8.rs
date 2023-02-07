use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryEncodeUTF8;
);
impl AlwaysAutofixableViolation for UnnecessaryEncodeUTF8 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary call to `encode` as UTF-8")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `encode`".to_string()
    }
}

const UTF8_LITERALS: &[&str] = &["utf-8", "utf8", "utf_8", "u8", "utf", "cp65001"];

fn match_encoded_variable(func: &Expr) -> Option<&Expr> {
    let ExprKind::Attribute {
        value: variable,
        attr,
        ..
    } = &func.node else {
        return None;
    };
    if attr != "encode" {
        return None;
    }
    Some(variable)
}

fn is_utf8_encoding_arg(arg: &Expr) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &arg.node
    {
        UTF8_LITERALS.contains(&value.to_lowercase().as_str())
    } else {
        false
    }
}

fn is_default_encode(args: &[Expr], kwargs: &[Keyword]) -> bool {
    match (args.len(), kwargs.len()) {
        // .encode()
        (0, 0) => true,
        // .encode(encoding)
        (1, 0) => is_utf8_encoding_arg(&args[0]),
        // .encode(kwarg=kwarg)
        (0, 1) => {
            kwargs[0].node.arg == Some("encoding".to_string())
                && is_utf8_encoding_arg(&kwargs[0].node.value)
        }
        // .encode(*args, **kwargs)
        _ => false,
    }
}

// Return a Fix for a default `encode` call removing the encoding argument,
// keyword, or positional.
fn delete_default_encode_arg_or_kwarg(
    expr: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
    patch: bool,
) -> Option<Diagnostic> {
    if let Some(arg) = args.get(0) {
        let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from_located(expr));
        if patch {
            diagnostic.amend(Fix::deletion(arg.location, arg.end_location.unwrap()));
        }
        Some(diagnostic)
    } else if let Some(kwarg) = kwargs.get(0) {
        let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from_located(expr));
        if patch {
            diagnostic.amend(Fix::deletion(kwarg.location, kwarg.end_location.unwrap()));
        }
        Some(diagnostic)
    } else {
        None
    }
}

// Return a Fix replacing the call to encode by a `"b"` prefix on the string.
fn replace_with_bytes_literal(
    expr: &Expr,
    constant: &Expr,
    locator: &Locator,
    patch: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from_located(expr));
    if patch {
        let content = locator.slice_source_code_range(&Range::new(
            constant.location,
            constant.end_location.unwrap(),
        ));
        let content = format!(
            "b{}",
            content.trim_start_matches('u').trim_start_matches('U')
        );
        diagnostic.amend(Fix::replacement(
            content,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    diagnostic
}

/// UP012
pub fn unnecessary_encode_utf8(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
) {
    let Some(variable) = match_encoded_variable(func) else {
        return;
    };
    match &variable.node {
        ExprKind::Constant {
            value: Constant::Str(literal),
            ..
        } => {
            // "str".encode()
            // "str".encode("utf-8")
            if is_default_encode(args, kwargs) {
                if literal.is_ascii() {
                    // "foo".encode()
                    checker.diagnostics.push(replace_with_bytes_literal(
                        expr,
                        variable,
                        checker.locator,
                        checker.patch(&Rule::UnnecessaryEncodeUTF8),
                    ));
                } else {
                    // "unicode text©".encode("utf-8")
                    if let Some(diagnostic) = delete_default_encode_arg_or_kwarg(
                        expr,
                        args,
                        kwargs,
                        checker.patch(&Rule::UnnecessaryEncodeUTF8),
                    ) {
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        // f"foo{bar}".encode(*args, **kwargs)
        ExprKind::JoinedStr { .. } => {
            if is_default_encode(args, kwargs) {
                if let Some(diagnostic) = delete_default_encode_arg_or_kwarg(
                    expr,
                    args,
                    kwargs,
                    checker.patch(&Rule::UnnecessaryEncodeUTF8),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        _ => {}
    }
}
