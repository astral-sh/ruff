use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{is_triple_quote, leading_quote};
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::TextRange;

use crate::lex::docstring_detection::StateMachine;

use crate::settings::LinterSettings;

/// ## What it does
/// Checks for strings that include escaped quotes, and suggests changing
/// the quote style to avoid the need to escape them.
///
/// ## Why is this bad?
/// It's preferable to avoid escaped quotes in strings. By changing the
/// outer quote style, you can avoid escaping inner quotes.
///
/// ## Example
/// ```python
/// foo = 'bar\'s'
/// ```
///
/// Use instead:
/// ```python
/// foo = "bar's"
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter automatically removes unnecessary escapes, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct AvoidableEscapedQuote;

impl AlwaysFixableViolation for AvoidableEscapedQuote {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Change outer quotes to avoid escaping inner quotes")
    }

    fn fix_title(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }
}

struct FStringContext {
    /// Whether to check for escaped quotes in the f-string.
    check_for_escaped_quote: bool,
    /// The range of the f-string start token.
    start_range: TextRange,
    /// The ranges of the f-string middle tokens containing escaped quotes.
    middle_ranges_with_escapes: Vec<TextRange>,
}

impl FStringContext {
    fn new(check_for_escaped_quote: bool, fstring_start_range: TextRange) -> Self {
        Self {
            check_for_escaped_quote,
            start_range: fstring_start_range,
            middle_ranges_with_escapes: vec![],
        }
    }

    /// Update the context to not check for escaped quotes, and clear any
    /// existing reported ranges.
    fn ignore_escaped_quotes(&mut self) {
        self.check_for_escaped_quote = false;
        self.middle_ranges_with_escapes.clear();
    }

    fn push_fstring_middle_range(&mut self, range: TextRange) {
        self.middle_ranges_with_escapes.push(range);
    }
}

/// Q003
pub(crate) fn avoidable_escaped_quote(
    diagnostics: &mut Vec<Diagnostic>,
    lxr: &[LexResult],
    locator: &Locator,
    settings: &LinterSettings,
) {
    let quotes_settings = &settings.flake8_quotes;
    let supports_pep701 = settings.target_version.supports_pep701();
    let mut fstrings: Vec<FStringContext> = Vec::new();
    let mut state_machine = StateMachine::default();

    for &(ref tok, tok_range) in lxr.iter().flatten() {
        let is_docstring = state_machine.consume(tok);
        if is_docstring {
            continue;
        }

        if !supports_pep701 {
            // If this is a string or a start of a f-string which is inside another
            // f-string, we won't check for escaped quotes for the entire f-string
            // if the target version doesn't support PEP 701. For example:
            //
            // ```python
            // f"\"foo\" {'nested'}"
            // #          ^^^^^^^^
            // #          We're here
            // ```
            //
            // If we try to fix the above example, the outer and inner quote
            // will be the same which is invalid pre 3.12:
            //
            // ```python
            // f'"foo" {'nested'}"
            // ```
            if matches!(tok, Tok::String { .. } | Tok::FStringStart) {
                if let Some(fstring_context) = fstrings.last_mut() {
                    fstring_context.ignore_escaped_quotes();
                    continue;
                }
            }
        }

        match tok {
            Tok::String {
                value: string_contents,
                kind,
                triple_quoted,
            } => {
                if kind.is_raw() || *triple_quoted {
                    continue;
                }

                // Check if we're using the preferred quotation style.
                if !leading_quote(locator.slice(tok_range))
                    .is_some_and(|text| text.contains(quotes_settings.inline_quotes.as_char()))
                {
                    continue;
                }

                if string_contents.contains(quotes_settings.inline_quotes.as_char())
                    && !string_contents.contains(quotes_settings.inline_quotes.opposite().as_char())
                {
                    let mut diagnostic = Diagnostic::new(AvoidableEscapedQuote, tok_range);
                    let fixed_contents = format!(
                        "{prefix}{quote}{value}{quote}",
                        prefix = kind.as_str(),
                        quote = quotes_settings.inline_quotes.opposite().as_char(),
                        value = unescape_string(string_contents)
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        fixed_contents,
                        tok_range,
                    )));
                    diagnostics.push(diagnostic);
                }
            }
            Tok::FStringStart => {
                let text = locator.slice(tok_range);
                // Check for escaped quote only if we're using the preferred quotation
                // style and it isn't a triple-quoted f-string.
                let check_for_escaped_quote = text
                    .contains(quotes_settings.inline_quotes.as_char())
                    && !is_triple_quote(text);
                fstrings.push(FStringContext::new(check_for_escaped_quote, tok_range));
            }
            Tok::FStringMiddle {
                value: string_contents,
                is_raw,
            } if !is_raw => {
                let Some(context) = fstrings.last_mut() else {
                    continue;
                };
                if !context.check_for_escaped_quote {
                    continue;
                }
                // If any part of the f-string contains the opposite quote,
                // we can't change the quote style in the entire f-string.
                if string_contents.contains(quotes_settings.inline_quotes.opposite().as_char()) {
                    context.ignore_escaped_quotes();
                    continue;
                }
                if string_contents.contains(quotes_settings.inline_quotes.as_char()) {
                    context.push_fstring_middle_range(tok_range);
                }
            }
            Tok::FStringEnd => {
                let Some(context) = fstrings.pop() else {
                    continue;
                };
                if context.middle_ranges_with_escapes.is_empty() {
                    // There are no `FStringMiddle` tokens containing any escaped
                    // quotes.
                    continue;
                }
                let mut diagnostic = Diagnostic::new(
                    AvoidableEscapedQuote,
                    TextRange::new(context.start_range.start(), tok_range.end()),
                );
                let fstring_start_edit = Edit::range_replacement(
                    // No need for `r`/`R` as we don't perform the checks
                    // for raw strings.
                    format!("f{}", quotes_settings.inline_quotes.opposite().as_char()),
                    context.start_range,
                );
                let fstring_middle_and_end_edits = context
                    .middle_ranges_with_escapes
                    .iter()
                    .map(|&range| {
                        Edit::range_replacement(unescape_string(locator.slice(range)), range)
                    })
                    .chain(std::iter::once(
                        // `FStringEnd` edit
                        Edit::range_replacement(
                            quotes_settings
                                .inline_quotes
                                .opposite()
                                .as_char()
                                .to_string(),
                            tok_range,
                        ),
                    ));
                diagnostic.set_fix(Fix::safe_edits(
                    fstring_start_edit,
                    fstring_middle_and_end_edits,
                ));
                diagnostics.push(diagnostic);
            }
            _ => {}
        }
    }
}

fn unescape_string(value: &str) -> String {
    let mut fixed_contents = String::with_capacity(value.len());

    let mut chars = value.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '\\' {
            fixed_contents.push(char);
            continue;
        }
        // If we're at the end of the line
        let Some(next_char) = chars.peek() else {
            fixed_contents.push(char);
            continue;
        };
        // Remove quote escape
        if matches!(*next_char, '\'' | '"') {
            continue;
        }
        fixed_contents.push(char);
    }

    fixed_contents
}
