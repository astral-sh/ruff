use std::str::FromStr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprStringLiteral, StringFlags, StringLiteral};
use ruff_python_literal::{
    cformat::{CFormatErrorType, CFormatString},
    format::FormatPart,
    format::FromTemplate,
    format::{FormatSpec, FormatSpecError, FormatString},
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unsupported format types in format strings.
///
/// ## Why is this bad?
/// An invalid format string character will result in an error at runtime.
///
/// ## Example
/// ```python
/// # `z` is not a valid format type.
/// print("%z" % "1")
///
/// print("{:z}".format("1"))
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BadStringFormatCharacter {
    format_char: char,
}

impl Violation for BadStringFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadStringFormatCharacter { format_char } = self;
        format!("Unsupported format character '{format_char}'")
    }
}

/// PLE1300
/// Ex) `"{:z}".format("1")`
pub(crate) fn call(checker: &Checker, string: &str, range: TextRange) {
    if let Ok(format_string) = FormatString::from_str(string) {
        for part in &format_string.format_parts {
            let FormatPart::Field { format_spec, .. } = part else {
                continue;
            };

            match FormatSpec::parse(format_spec) {
                Err(FormatSpecError::InvalidFormatType) => {
                    checker.report_diagnostic(Diagnostic::new(
                        BadStringFormatCharacter {
                            // The format type character is always the last one.
                            // More info in the official spec:
                            // https://docs.python.org/3/library/string.html#format-specification-mini-language
                            format_char: format_spec.chars().last().unwrap(),
                        },
                        range,
                    ));
                }
                Err(_) => {}
                Ok(FormatSpec::Static(_)) => {}
                Ok(FormatSpec::Dynamic(format_spec)) => {
                    for placeholder in format_spec.placeholders {
                        let FormatPart::Field { format_spec, .. } = placeholder else {
                            continue;
                        };
                        if let Err(FormatSpecError::InvalidFormatType) =
                            FormatSpec::parse(&format_spec)
                        {
                            checker.report_diagnostic(Diagnostic::new(
                                BadStringFormatCharacter {
                                    // The format type character is always the last one.
                                    // More info in the official spec:
                                    // https://docs.python.org/3/library/string.html#format-specification-mini-language
                                    format_char: format_spec.chars().last().unwrap(),
                                },
                                range,
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// PLE1300
/// Ex) `"%z" % "1"`
pub(crate) fn percent(checker: &Checker, expr: &Expr, format_string: &ExprStringLiteral) {
    for StringLiteral {
        value: _,
        range,
        flags,
    } in &format_string.value
    {
        let string = checker.locator().slice(range);
        let string = &string
            [usize::from(flags.opener_len())..(string.len() - usize::from(flags.closer_len()))];

        // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
        if let Err(format_error) = CFormatString::from_str(string) {
            if let CFormatErrorType::UnsupportedFormatChar(format_char) = format_error.typ {
                checker.report_diagnostic(Diagnostic::new(
                    BadStringFormatCharacter { format_char },
                    expr.range(),
                ));
            }
        }
    }
}
