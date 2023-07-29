use std::str::FromStr;

use ruff_python_ast::{Constant, Expr, ExprBinOp, ExprConstant, Operator, Ranged};
use ruff_python_literal::{
    cformat::{CFormatErrorType, CFormatString},
    format::FormatPart,
    format::FromTemplate,
    format::{FormatSpec, FormatSpecError, FormatString},
};
use ruff_python_parser::{lexer, Mode};
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unsupported argument types in format strings.
///
/// ## Why is this bad?
/// The format string is not checked at compile time, so it is easy to
/// introduce bugs by mistyping the format string.
///
/// ## Example
/// ```python
/// print("%z" % "1")
///
/// print("{:z}".format("1"))
///
/// x = 1
/// print(f"{x:z}")
/// ```
#[violation]
pub struct BadFormatCharacter {
    format_char: char,
    index: usize,
}

impl Violation for BadFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Unsupported format character '{}' ({:#x}) at index {}",
            self.format_char, self.format_char as u8, self.index
        )
    }
}

fn bad_format_character_in_new_style_format(checker: &mut Checker, string: &str, range: TextRange) {
    if let Ok(format_string) = FormatString::from_str(string) {
        // We keep track of the length of the format string so we can report the correct index
        // in the string literal.
        let mut parts_len = 0;
        for part in &format_string.format_parts {
            match part {
                FormatPart::Field { format_spec, .. } => {
                    // Add two for the opening brace and the colon {:
                    parts_len += 2 + format_spec.len();
                    if let Err(FormatSpecError::InvalidFormatType) = FormatSpec::parse(format_spec)
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            BadFormatCharacter {
                                // The format type character is always the last one.
                                // More info in the official spec:
                                // https://docs.python.org/3/library/string.html#format-specification-mini-language
                                format_char: format_spec.chars().last().unwrap(),
                                index: parts_len - 1,
                            },
                            range,
                        ));
                    }
                    // Add 1 for the closing brace }
                    parts_len += 1;
                }
                FormatPart::Literal(s) => parts_len += s.len(),
            }
        }
    }
}

/// PLE1300
fn bad_format_character_in_old_style_format(checker: &mut Checker, expr: &Expr) {
    // Grab each string segment (in case there's an implicit concatenation).
    let content = checker.locator().slice(expr.range());
    let mut strings: Vec<TextRange> = vec![];
    for (tok, range) in lexer::lex_starts_at(content, Mode::Module, expr.start()).flatten() {
        if tok.is_string() {
            strings.push(range);
        } else if tok.is_percent() {
            // Break as soon as we find the modulo symbol.
            break;
        }
    }

    // If there are no string segments, abort.
    if strings.is_empty() {
        return;
    }
    for range in &strings {
        let string = checker.locator().slice(*range);
        let (Some(leader), Some(trailer)) = (leading_quote(string), trailing_quote(string)) else {
            return;
        };
        let string = &string[leader.len()..string.len() - trailer.len()];

        // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
        if let Err(format_error) = CFormatString::from_str(string) {
            match format_error.typ {
                CFormatErrorType::UnsupportedFormatChar(c) => {
                    checker.diagnostics.push(Diagnostic::new(
                        BadFormatCharacter {
                            format_char: c,
                            index: format_error.index,
                        },
                        expr.range(),
                    ));
                }
                _ => continue,
            }
        };
    }
}

pub(crate) fn bad_format_character(checker: &mut Checker, expr: &Expr) {
    match expr {
        Expr::Constant(ExprConstant {
            value: Constant::Str(value),
            kind: _,
            range: _,
        }) => bad_format_character_in_new_style_format(checker, value, expr.range()),
        Expr::BinOp(ExprBinOp {
            left: _,
            op: Operator::Mod,
            right: _,
            range: _,
        }) => {
            bad_format_character_in_old_style_format(checker, expr);
        }
        _ => {}
    }
}
