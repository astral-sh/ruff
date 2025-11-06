use std::fmt::Write as _;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword, StringLiteral, StringLiteralValue};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_python_trivia::Cursor;
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, pad, remove_argument};
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155")]
pub(crate) struct UnnecessaryEncodeUTF8 {
    reason: Reason,
}

impl AlwaysFixableViolation for UnnecessaryEncodeUTF8 {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.reason {
            Reason::BytesLiteral => "Unnecessary call to `encode` as UTF-8".to_string(),
            Reason::DefaultArgument => {
                "Unnecessary UTF-8 `encoding` argument to `encode`".to_string()
            }
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
        UTF8_LITERALS.contains(&value.to_str().to_lowercase().as_str())
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
fn match_encoding_arg(arguments: &Arguments) -> Option<EncodingArg<'_>> {
    match (&*arguments.args, &*arguments.keywords) {
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
fn replace_with_bytes_literal(locator: &Locator, call: &ast::ExprCall, tokens: &Tokens) -> Fix {
    // Build up a replacement string by prefixing all string tokens with `b`.
    let mut replacement = String::with_capacity(call.range().len().to_usize() + 1);
    let mut prev = call.start();
    for token in tokens.in_range(call.range()) {
        match token.kind() {
            TokenKind::Dot => break,
            TokenKind::String => {
                replacement.push_str(locator.slice(TextRange::new(prev, token.start())));
                let string = locator.slice(token);
                let _ = write!(
                    &mut replacement,
                    "b{}",
                    &string.trim_start_matches('u').trim_start_matches('U')
                );
            }
            _ => {
                replacement.push_str(locator.slice(TextRange::new(prev, token.end())));
            }
        }
        prev = token.end();
    }

    Fix::safe_edit(Edit::range_replacement(
        pad(replacement, call.range(), locator),
        call.range(),
    ))
}

/// UP012
pub(crate) fn unnecessary_encode_utf8(checker: &Checker, call: &ast::ExprCall) {
    let Some(variable) = match_encoded_variable(&call.func) else {
        return;
    };
    match variable {
        Expr::StringLiteral(ast::ExprStringLiteral { value: literal, .. }) => {
            if string_contains_string_only_escapes(literal, checker.locator()) {
                return;
            }

            // Ex) `"str".encode()`, `"str".encode("utf-8")`
            if let Some(encoding_arg) = match_encoding_arg(&call.arguments) {
                if literal.to_str().is_ascii() {
                    // Ex) Convert `"foo".encode()` to `b"foo"`.
                    let mut diagnostic = checker.report_diagnostic(
                        UnnecessaryEncodeUTF8 {
                            reason: Reason::BytesLiteral,
                        },
                        call.range(),
                    );
                    diagnostic.set_fix(replace_with_bytes_literal(
                        checker.locator(),
                        call,
                        checker.tokens(),
                    ));
                } else if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode(encoding="utf-8")` to
                    // `"unicode text©".encode()`.
                    let mut diagnostic = checker.report_diagnostic(
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
                            checker.comment_ranges(),
                        )
                        .map(Fix::safe_edit)
                    });
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `"unicode text©".encode("utf-8")` to `"unicode text©".encode()`.
                    let mut diagnostic = checker.report_diagnostic(
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
                            checker.comment_ranges(),
                        )
                        .map(Fix::safe_edit)
                    });
                }
            }
        }
        // Ex) `f"foo{bar}".encode("utf-8")`
        Expr::FString(_) => {
            if let Some(encoding_arg) = match_encoding_arg(&call.arguments) {
                if let EncodingArg::Keyword(kwarg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode(encoding="utf-8")` to
                    // `f"unicode text©".encode()`.
                    let mut diagnostic = checker.report_diagnostic(
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
                            checker.comment_ranges(),
                        )
                        .map(Fix::safe_edit)
                    });
                } else if let EncodingArg::Positional(arg) = encoding_arg {
                    // Ex) Convert `f"unicode text©".encode("utf-8")` to `f"unicode text©".encode()`.
                    let mut diagnostic = checker.report_diagnostic(
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
                            checker.comment_ranges(),
                        )
                        .map(Fix::safe_edit)
                    });
                }
            }
        }
        _ => {}
    }
}
/// In a string, there are two kinds of escape sequences: "single" and "multi".
///
/// A "single" escape sequence is formed if a backslash is followed by
/// a newline, another backslash, `'`, `"`, `a`, `b`, `f`, `n`, `t`, or `v`.
/// A "multi" escape sequence is formed if a backslash is followed by
/// `x` and 2 hex digits, `N` and a Unicode character name enclosed in a pair of braces,
/// `u` and 4 hex digits, `U` and 8 hex digits, or 1 to 3 oct digits.
///
/// Out of the aforementioned, `u`, `U` and `N` are only valid in a string.
/// However, an octal escape `\ooo` where `ooo` is greater than 377 base 8
/// currently raises a `SyntaxWarning` (will eventually be a `SyntaxError`)
/// in both strings and bytes and thus is not considered `bytes`-compatible.
///
/// An unrecognized escape sequence is ignored, resulting in both
/// the backslash and the following character being part of the string.
///
/// Reference: [Lexical analysis &sect; 2.4.1.1. Escape sequences][escape-sequences]
///
/// [escape-sequences]: https://docs.python.org/3/reference/lexical_analysis.html#escape-sequences
fn string_contains_string_only_escapes(string: &StringLiteralValue, locator: &Locator) -> bool {
    for literal in string {
        let flags = literal.flags;

        if flags.prefix().is_raw() {
            continue;
        }

        if literal.content_range().len() > literal.as_str().text_len()
            && literal_contains_string_only_escapes(literal, locator)
        {
            return true;
        }
    }

    false
}

fn literal_contains_string_only_escapes(literal: &StringLiteral, locator: &Locator) -> bool {
    let inner_in_source = locator.slice(literal.content_range());

    let mut cursor = Cursor::new(inner_in_source);

    while let Some(backslash_offset) = memchr::memchr(b'\\', cursor.as_bytes()) {
        cursor.skip_bytes(backslash_offset + "\\".len());

        let Some(escaped) = cursor.bump() else {
            continue;
        };

        match escaped {
            'N' | 'u' | 'U' => return true,
            'x' => {
                cursor.skip_bytes(2);
            }
            '0'..='7' => {
                let (second, third) = (cursor.first(), cursor.second());

                let octal_codepoint = match (is_octal_digit(second), is_octal_digit(third)) {
                    (false, _) => escaped.to_string(),
                    (true, false) => format!("{escaped}{second}"),
                    (true, true) => format!("{escaped}{second}{third}"),
                };

                if octal_codepoint.parse::<u8>().is_err() {
                    return true;
                }

                cursor.skip_bytes(octal_codepoint.len());
            }
            _ => {}
        }
    }

    false
}

const fn is_octal_digit(char: char) -> bool {
    matches!(char, '0'..='7')
}
