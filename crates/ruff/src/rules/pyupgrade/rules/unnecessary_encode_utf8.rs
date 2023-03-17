use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct UnnecessaryEncodeUTF8;

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
            kwargs[0].node.arg.as_ref().unwrap() == "encoding"
                && is_utf8_encoding_arg(&kwargs[0].node.value)
        }
        // .encode(*args, **kwargs)
        _ => false,
    }
}

/// Return a [`Fix`] for a default `encode` call removing the encoding argument,
/// keyword, or positional.
fn delete_default_encode_arg_or_kwarg(
    expr: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
    patch: bool,
) -> Option<Diagnostic> {
    if let Some(arg) = args.get(0) {
        let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from(expr));
        if patch {
            diagnostic.amend(Fix::deletion(arg.location, arg.end_location.unwrap()));
        }
        Some(diagnostic)
    } else if let Some(kwarg) = kwargs.get(0) {
        let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from(expr));
        if patch {
            diagnostic.amend(Fix::deletion(kwarg.location, kwarg.end_location.unwrap()));
        }
        Some(diagnostic)
    } else {
        None
    }
}

/// Return a [`Fix`] replacing the call to encode by a `"b"` prefix on the string.
fn replace_with_bytes_literal(
    expr: &Expr,
    constant: &Expr,
    locator: &Locator,
    patch: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(UnnecessaryEncodeUTF8, Range::from(expr));
    if patch {
        // Build up a replacement string by prefixing all string tokens with `b`.
        let contents = locator.slice(Range::new(
            constant.location,
            constant.end_location.unwrap(),
        ));
        let mut replacement = String::with_capacity(contents.len() + 1);
        let mut prev = None;
        for (start, tok, end) in
            lexer::lex_located(contents, Mode::Module, constant.location).flatten()
        {
            if matches!(tok, Tok::String { .. }) {
                if let Some(prev) = prev {
                    replacement.push_str(locator.slice(Range::new(prev, start)));
                }
                let string = locator.slice(Range::new(start, end));
                replacement.push_str(&format!(
                    "b{}",
                    &string.trim_start_matches('u').trim_start_matches('U')
                ));
            } else {
                if let Some(prev) = prev {
                    replacement.push_str(locator.slice(Range::new(prev, end)));
                }
            }
            prev = Some(end);
        }
        diagnostic.amend(Fix::replacement(
            replacement,
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
                        checker.patch(Rule::UnnecessaryEncodeUTF8),
                    ));
                } else {
                    // "unicode text©".encode("utf-8")
                    if let Some(diagnostic) = delete_default_encode_arg_or_kwarg(
                        expr,
                        args,
                        kwargs,
                        checker.patch(Rule::UnnecessaryEncodeUTF8),
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
                    checker.patch(Rule::UnnecessaryEncodeUTF8),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        _ => {}
    }
}
