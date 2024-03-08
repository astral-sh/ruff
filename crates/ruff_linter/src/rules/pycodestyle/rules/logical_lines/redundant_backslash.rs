use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, LogicalLineToken};

/// ## What it does
/// Checks for redundant backslashes between brackets.
///
/// ## Why is this bad?
/// Explicit line joins using a backslash are redundant between brackets.
///
/// ## Example
/// ```python
/// x = (2 + \
///     2)
/// ```
///
/// Use instead:
/// ```python
/// x = (2 +
///     2)
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
#[violation]
pub struct RedundantBackslash;

impl AlwaysFixableViolation for RedundantBackslash {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Redundant backslash")
    }

    fn fix_title(&self) -> String {
        "Remove redundant backslash".to_string()
    }
}

fn neuter_strings(text: &str, tokens: &[LogicalLineToken]) -> String {
    let offset = tokens.first().unwrap().start().to_usize();
    let mut new_text = String::with_capacity(text.len());
    let mut last_end = 0;

    for token in tokens {
        if matches!(token.kind(), TokenKind::String | TokenKind::FStringMiddle) {
            new_text.push_str(&text[last_end..token.start().to_usize() - offset]);
            let token_text =
                &text[token.start().to_usize() - offset..token.end().to_usize() - offset];
            new_text.push_str(&token_text.replace('\\', " "));
            last_end = token.end().to_usize() - offset;
        }
    }

    new_text.push_str(&text[last_end..]);
    new_text
}

/// E502
pub(crate) fn redundant_backslash(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut text = line.text().to_string();
    let mut cursor = line.tokens().first().unwrap().start().to_usize();
    let mut start = 0;
    let mut parens = 0;
    let mut comment = false;
    let mut backslash = false;

    text = neuter_strings(&text, line.tokens());

    for c in text.chars() {
        match c {
            '#' => {
                backslash = false;
                comment = true;
            }
            '\r' | '\n' => {
                if !comment && backslash && parens > 0 {
                    let start_s = TextSize::new(u32::try_from(start).unwrap());
                    let end_s = TextSize::new(u32::try_from(cursor).unwrap());
                    let mut diagnostic =
                        Diagnostic::new(RedundantBackslash, TextRange::new(start_s, end_s));
                    diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start_s, end_s)));
                    context.push_diagnostic(diagnostic);
                }
                backslash = false;
                comment = false;
            }
            '(' | '[' | '{' => {
                backslash = false;
                if !comment {
                    parens += 1;
                }
            }
            ')' | ']' | '}' => {
                backslash = false;
                if !comment {
                    parens -= 1;
                }
            }
            '\\' => {
                start = cursor;
                backslash = true;
            }
            _ => {
                backslash = false;
            }
        }
        cursor += c.len_utf8();
    }
}
