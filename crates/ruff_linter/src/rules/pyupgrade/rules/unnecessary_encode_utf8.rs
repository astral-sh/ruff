use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword, PySourceType};
use ruff_python_parser::{lexer, AsMode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::{pad, remove_argument, Parentheses};

/// ## What it does
/// Checks for unnecessary calls to `encode` as UTF-8.
///
/// ## Why is this bad?
/// UTF-8 is the default encoding in Python, so there is no need to call
/// `encode` when UTF-8 is the desired encoding. Instead, use a bytes literal.
///
/// ## Example
/// ```python
/// "foo".encode("utf-8")
/// ```
///
/// Use instead:
/// ```python
/// b"foo"
/// ```
///
/// ## References
/// - [Python documentation: `str.encode`](https://docs.python.org/3/library/stdtypes.html#str.encode)
#[violation]
pub struct UnnecessaryEncodeUTF8 {
    reason: Reason,
}

impl AlwaysFixableViolation for UnnecessaryEncodeUTF8 {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.reason {
            Reason::BytesLiteral => format!("Unnecessary call to `encode` as UTF-8"),
            Reason::DefaultArgument => format!("Unnecessary UTF-8 `encoding` argument to `encode`"),
        }
    }

    fn fix_title(&self) -> String {
        match self.reason {
            Reason::BytesLiteral => "Rewrite as bytes literal".to_string(),
            Reason::DefaultArgument => "Remove unnecessary `encoding` argument".to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Reason {
    BytesLiteral,
    DefaultArgument,
}

const UTF8_LITERALS: &[&str] = &["utf-8", "utf8", "utf_8", "u8", "utf", "cp65001"];

fn match_encoded_variable(func: &Expr) -> Option<&Expr> {
    let Expr::Attribute(ast::ExprAttribute {
        value: variable,
        attr,
        ..
    }) = func
    else {
        return None;
    };
    if attr != "encode" {
        return None;
    }
    Some(variable)
}

fn is_utf8_encoding_arg(arg: &Expr) -> bool {
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &arg {
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
fn match_encoding_arg(arguments: &Arguments) -> Option<EncodingArg> {
    match (arguments.args.as_slice(), arguments.keywords.as_slice()) {
        // Ex `"".encode()`
        ([], []) => return Some(EncodingArg::Empty),
        // Ex `"".encode(encoding)`
        ([arg], []) => {
            if is_utf8_encoding_arg(arg) {
                return Some(EncodingArg::Positional(arg));
            }
        }
        // Ex `"".encode(kwarg=kwarg)`
        ([], [keyword]) => {
            if keyword.arg.as_ref().is_some_and(|arg| arg == "encoding") {
                if is_utf8_encoding_arg(&keyword.value) {
                    return Some(EncodingArg::Keyword(keyword));
                }
            }
        }
        // Ex `"".encode(*args, **kwargs)`
        _ => {}
    }
    None
}

/// Return a [`Fix`] replacing the call to encode with a byte string.
fn replace_with_bytes_literal(
    locator: &Locator,
    call: &ast::ExprCall,
    source_type: PySourceType,
) -> Fix {
    // Build up a replacement string by prefixing all string tokens with `b`.
    let contents = locator.slice(call);
    let mut replacement = String::with_capacity(contents.len() + 1);
    let mut prev = call.start();
    for (tok, range) in
        lexer::lex_starts_at(contents, source_type.as_mode(), call.start()).flatten()
    {
        match tok {
            Tok::Dot => break,
            Tok::String { .. } => {
                replacement.push_str(locator.slice(TextRange::new(prev, range.start())));
                let string = locator.slice(range);
                replacement.push_str(&format!(
                    "b{}",
                    &string.trim_start_matches('u').trim_start_matches('U')
                ));
            }
            _ => {
                replacement.push_str(locator.slice(TextRange::new(prev, range.end())));
            }
        }
        prev = range.end();
    }

    Fix::safe_edit(Edit::range_replacement(
        pad(replacement, call.range(), locator),
        call.range(),
    ))
}

/// UP012
pub(crate) fn unnecessary_encode_utf8(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(variable) = match_encoded_variable(&call.func) else {
        return;
    };
    match variable {
        Expr::StringLiteral(ast::ExprStringLiteral { value: literal, .. }) => {
            // Ex) `"str".encode()`, `"str".encode("utf-8")`
            if let Some(encoding_arg) = match_encoding_arg(&call.arguments) {
                if literal.is_ascii() {
                    // Ex) Convert `"foo".encode()` to `b"foo"`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::BytesLiteral,
                        },
                        call.range(),
                    );
                    diagnostic.set_fix(replace_with_bytes_literal(
                        checker.locator(),
                        call,
                        checker.source_type,
                    ));
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode(encoding="utf-8")` to
                    // `"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        call.range(),
                    );
                    diagnostic.try_set_fix(|| {
                        remove_argument(
                            kwarg,
                            &call.arguments,
                            Parentheses::Preserve,
                            checker.locator().contents(),
                        )
                        .map(Fix::safe_edit)
                    });
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode("utf-8")` to `"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        call.range(),
                    );
                    diagnostic.try_set_fix(|| {
                        remove_argument(
                            arg,
                            &call.arguments,
                            Parentheses::Preserve,
                            checker.locator().contents(),
                        )
                        .map(Fix::safe_edit)
                    });
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        // Ex) `f"foo{bar}".encode("utf-8")`
        Expr::FString(_) => {
            if let Some(encoding_arg) = match_encoding_arg(&call.arguments) {
                if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode(encoding="utf-8")` to
                    // `f"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        call.range(),
                    );
                    diagnostic.try_set_fix(|| {
                        remove_argument(
                            kwarg,
                            &call.arguments,
                            Parentheses::Preserve,
                            checker.locator().contents(),
                        )
                        .map(Fix::safe_edit)
                    });
                    checker.diagnostics.push(diagnostic);
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode("utf-8")` to `f"unicode text©".encode()`.
                    let mut diagnostic = Diagnostic::new(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::DefaultArgument,
                        },
                        call.range(),
                    );
                    diagnostic.try_set_fix(|| {
                        remove_argument(
                            arg,
                            &call.arguments,
                            Parentheses::Preserve,
                            checker.locator().contents(),
                        )
                        .map(Fix::safe_edit)
                    });
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        _ => {}
    }
}
