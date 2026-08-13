use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_literal::{
    format::FormatPart,
    format::FromTemplate,
    format::{FormatSpec, FormatSpecError, FormatString},
};
use ruff_text_size::TextRange;

use crate::Violation;
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
#[violation_metadata(stable_since = "v0.0.283")]
pub(crate) struct BadStringFormatCharacter {
    pub(crate) format_char: char,
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
                Err(FormatSpecError::InvalidFormatType(format_char)) => {
                    checker.report_diagnostic(BadStringFormatCharacter { format_char }, range);
                }
                Err(_) => {}
                Ok(FormatSpec::Static(_)) => {}
                Ok(FormatSpec::Dynamic(format_spec)) => {
                    for placeholder in format_spec.placeholders {
                        let FormatPart::Field { format_spec, .. } = placeholder else {
                            continue;
                        };
                        if let Err(FormatSpecError::InvalidFormatType(format_char)) =
                            FormatSpec::parse(&format_spec)
                        {
                            checker
                                .report_diagnostic(BadStringFormatCharacter { format_char }, range);
                        }
                    }
                }
            }
        }
    }
}
