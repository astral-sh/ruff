use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::autofix::actions::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[derive(Debug, PartialEq, Eq)]
pub enum Reason {
    BytesLiteral,
    DefaultArgument,
}

#[violation]
pub struct UnnecessaryEncodeUTF8 {
    pub reason: Reason,
}

impl AlwaysAutofixableViolation for UnnecessaryEncodeUTF8 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary call to `encode` as UTF-8")
    }

    fn autofix_title(&self) -> String {
        match self.reason {
            Reason::BytesLiteral => "Rewrite as bytes literal".to_string(),
            Reason::DefaultArgument => "Remove unnecessary encoding argument".to_string(),
        }
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

#[derive(Debug)]
enum EncodingArg<'a> {
    /// Ex) `"".encode()`
    Empty,
    /// Ex) `"".encode("utf-8")`
    Positional(&'a Expr),
    /// Ex) `"".encode(encoding="utf-8")`
    Keyword(&'a Keyword),
}

/// Return the encoding argument to an `encode` call, if it can be determined to be a
/// UTF-8-equivalent encoding.
fn match_encoding_arg<'a>(args: &'a [Expr], kwargs: &'a [Keyword]) -> Option<EncodingArg<'a>> {
    match (args.len(), kwargs.len()) {
        // Ex `"".encode()`
        (0, 0) => return Some(EncodingArg::Empty),
        // Ex `"".encode(encoding)`
        (1, 0) => {
            let arg = &args[0];
            if is_utf8_encoding_arg(arg) {
                return Some(EncodingArg::Positional(arg));
            }
        }
        // Ex `"".encode(kwarg=kwarg)`
        (0, 1) => {
            let kwarg = &kwargs[0];
            if kwarg
                .node
                .arg
                .as_ref()
                .map_or(false, |arg| arg == "encoding")
            {
                if is_utf8_encoding_arg(&kwarg.node.value) {
                    return Some(EncodingArg::Keyword(kwarg));
                }
            }
        }
        // Ex `"".encode(*args, **kwargs)`
        _ => {}
    }
    None
}

/// Return an [`Edit`] replacing the call to encode with a byte string.
fn replace_with_bytes_literal(locator: &Locator, expr: &Expr, constant: &Expr) -> Edit {
    // Build up a replacement string by prefixing all string tokens with `b`.
    let contents = locator.slice(constant);
    let mut replacement = String::with_capacity(contents.len() + 1);
    let mut prev = None;
    for (start, tok, end) in lexer::lex_located(contents, Mode::Module, constant.location).flatten()
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
    Edit::replacement(replacement, expr.location, expr.end_location.unwrap())
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
            // Ex) `"str".encode()`, `"str".encode("utf-8")`
            if let Some(encoding_arg) = match_encoding_arg(args, kwargs) {
                if literal.is_ascii() {
                    // Ex) Convert `"foo".encode()` to `b"foo"`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::BytesLiteral,
                        },
                        Range::from(expr),
                    );
                    if checker.patch(Rule::UnnecessaryEncodeUTF8) {
                        diagnostic.set_fix(replace_with_bytes_literal(
                            checker.locator,
                            expr,
                            variable,
                        ));
                    }
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode(encoding="utf-8")` to
                    // `"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        Range::from(expr),
                    );
                    if checker.patch(Rule::UnnecessaryEncodeUTF8) {
                        diagnostic.try_set_fix(|| {
                            remove_argument(
                                checker.locator,
                                func.location,
                                kwarg.location,
                                kwarg.end_location.unwrap(),
                                args,
                                kwargs,
                                false,
                            )
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode("utf-8")` to `"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        Range::from(expr),
                    );
                    if checker.patch(Rule::UnnecessaryEncodeUTF8) {
                        diagnostic.try_set_fix(|| {
                            remove_argument(
                                checker.locator,
                                func.location,
                                arg.location,
                                arg.end_location.unwrap(),
                                args,
                                kwargs,
                                false,
                            )
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        // Ex) `f"foo{bar}".encode("utf-8")`
        ExprKind::JoinedStr { .. } => {
            if let Some(encoding_arg) = match_encoding_arg(args, kwargs) {
                if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode(encoding="utf-8")` to
                    // `f"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        Range::from(expr),
                    );
                    if checker.patch(Rule::UnnecessaryEncodeUTF8) {
                        diagnostic.try_set_fix(|| {
                            remove_argument(
                                checker.locator,
                                func.location,
                                kwarg.location,
                                kwarg.end_location.unwrap(),
                                args,
                                kwargs,
                                false,
                            )
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode("utf-8")` to `f"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        Range::from(expr),
                    );
                    if checker.patch(Rule::UnnecessaryEncodeUTF8) {
                        diagnostic.try_set_fix(|| {
                            remove_argument(
                                checker.locator,
                                func.location,
                                arg.location,
                                arg.end_location.unwrap(),
                                args,
                                kwargs,
                                false,
                            )
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        _ => {}
    }
}
