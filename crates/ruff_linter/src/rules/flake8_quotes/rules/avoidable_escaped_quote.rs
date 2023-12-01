use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{is_triple_quote, leading_quote};
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::TextRange;

use crate::lex::docstring_detection::StateMachine;
use crate::settings::LinterSettings;

use super::super::settings::Quote;

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

/// ## What it does
/// Checks for strings that include unnecessarily escaped quotes.
///
/// ## Why is this bad?
/// If a string contains an escaped quote that doesn't match the quote
/// character used for the string, it's unnecessary and can be removed.
///
/// ## Example
/// ```python
/// foo = "bar\'s"
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
pub struct UnnecessaryEscapedQuote;

impl AlwaysFixableViolation for UnnecessaryEscapedQuote {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary escape on inner quote character")
    }

    fn fix_title(&self) -> String {
        "Remove backslash".to_string()
    }
}

struct FStringContext {
    /// Whether to check for escaped quotes in the f-string.
    check_for_escaped_quote: bool,
    /// The range of the f-string start token.
    start_range: TextRange,
    /// The ranges of the f-string middle tokens containing escaped quotes.
    middle_ranges_with_escapes: Vec<TextRange>,
    /// The quote style used for the f-string
    quote_style: Quote,
}

impl FStringContext {
    fn new(
        check_for_escaped_quote: bool,
        fstring_start_range: TextRange,
        quote_style: Quote,
    ) -> Self {
        Self {
            check_for_escaped_quote,
            start_range: fstring_start_range,
            middle_ranges_with_escapes: vec![],
            quote_style,
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
                if !leading_quote(locator.slice(tok_range)).is_some_and(|text| {
                    contains_quote(text, quotes_settings.inline_quotes.as_char())
                }) {
                    continue;
                }

                if contains_escaped_quote(string_contents, quotes_settings.inline_quotes.as_char())
                    && !contains_quote(
                        string_contents,
                        quotes_settings.inline_quotes.opposite().as_char(),
                    )
                {
                    let mut diagnostic = Diagnostic::new(AvoidableEscapedQuote, tok_range);
                    let fixed_contents = format!(
                        "{prefix}{quote}{value}{quote}",
                        prefix = kind.as_str(),
                        quote = quotes_settings.inline_quotes.opposite().as_char(),
                        value = unescape_string(
                            string_contents,
                            quotes_settings.inline_quotes.as_char()
                        )
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
                let check_for_escaped_quote = !is_triple_quote(text)
                    && contains_quote(text, quotes_settings.inline_quotes.as_char());
                fstrings.push(FStringContext::new(
                    check_for_escaped_quote,
                    tok_range,
                    quotes_settings.inline_quotes,
                ));
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
                if contains_quote(
                    string_contents,
                    quotes_settings.inline_quotes.opposite().as_char(),
                ) {
                    context.ignore_escaped_quotes();
                    continue;
                }
                if contains_escaped_quote(string_contents, quotes_settings.inline_quotes.as_char())
                {
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
                        Edit::range_replacement(
                            unescape_string(
                                locator.slice(range),
                                quotes_settings.inline_quotes.as_char(),
                            ),
                            range,
                        )
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

/// Q004
pub(crate) fn unnecessary_escaped_quote(
    diagnostics: &mut Vec<Diagnostic>,
    lxr: &[LexResult],
    locator: &Locator,
) {
    let mut fstrings: Vec<FStringContext> = Vec::new();
    let mut state_machine = StateMachine::default();

    for &(ref tok, tok_range) in lxr.iter().flatten() {
        let is_docstring = state_machine.consume(tok);
        if is_docstring {
            continue;
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

                let leading = match leading_quote(locator.slice(tok_range)) {
                    Some("\"") => Quote::Double,
                    Some("'") => Quote::Single,
                    _ => continue,
                };
                if !contains_escaped_quote(string_contents, leading.opposite().as_char()) {
                    continue;
                }

                let mut diagnostic = Diagnostic::new(UnnecessaryEscapedQuote, tok_range);
                let fixed_contents = format!(
                    "{prefix}{quote}{value}{quote}",
                    prefix = kind.as_str(),
                    quote = leading.as_char(),
                    value = unescape_string(string_contents, leading.opposite().as_char())
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    fixed_contents,
                    tok_range,
                )));
                diagnostics.push(diagnostic);
            }
            Tok::FStringStart => {
                let text = locator.slice(tok_range);
                // Check for escaped quote only if we're using the preferred quotation
                // style and it isn't a triple-quoted f-string.
                let check_for_escaped_quote = !is_triple_quote(text);
                let quote_style = if contains_quote(text, Quote::Single.as_char()) {
                    Quote::Single
                } else {
                    Quote::Double
                };
                fstrings.push(FStringContext::new(
                    check_for_escaped_quote,
                    tok_range,
                    quote_style,
                ));
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
                if contains_escaped_quote(string_contents, context.quote_style.opposite().as_char())
                {
                    context.push_fstring_middle_range(tok_range);
                }
            }
            Tok::FStringEnd => {
                let Some(context) = fstrings.pop() else {
                    continue;
                };
                let [first, rest @ ..] = context.middle_ranges_with_escapes.as_slice() else {
                    continue;
                };
                let mut diagnostic = Diagnostic::new(
                    UnnecessaryEscapedQuote,
                    TextRange::new(context.start_range.start(), tok_range.end()),
                );
                let first_edit = Edit::range_replacement(
                    unescape_string(
                        locator.slice(first),
                        context.quote_style.opposite().as_char(),
                    ),
                    *first,
                );
                let rest_edits = rest.iter().map(|&range| {
                    Edit::range_replacement(
                        unescape_string(
                            locator.slice(range),
                            context.quote_style.opposite().as_char(),
                        ),
                        range,
                    )
                });
                diagnostic.set_fix(Fix::safe_edits(first_edit, rest_edits));
                diagnostics.push(diagnostic);
            }
            _ => {}
        }
    }
}

/// Return `true` if the haystack contains the quote.
fn contains_quote(haystack: &str, quote: char) -> bool {
    memchr::memchr(quote as u8, haystack.as_bytes()).is_some()
}

/// Return `true` if the haystack contains an escaped quote.
fn contains_escaped_quote(haystack: &str, quote: char) -> bool {
    for index in memchr::memchr_iter(quote as u8, haystack.as_bytes()) {
        // If the quote is preceded by an even number of backslashes, it's not escaped.
        if haystack.as_bytes()[..index]
            .iter()
            .rev()
            .take_while(|&&c| c == b'\\')
            .count()
            % 2
            != 0
        {
            return true;
        }
    }
    false
}

/// Return a modified version of the string with all quote escapes removed.
fn unescape_string(haystack: &str, quote: char) -> String {
    let mut fixed_contents = String::with_capacity(haystack.len());

    let mut chars = haystack.chars().peekable();
    let mut backslashes = 0;
    while let Some(char) = chars.next() {
        if char != '\\' {
            fixed_contents.push(char);
            backslashes = 0;
            continue;
        }
        // If we're at the end of the line
        let Some(next_char) = chars.peek() else {
            fixed_contents.push(char);
            continue;
        };
        // Remove quote escape
        if *next_char == quote && backslashes % 2 == 0 {
            backslashes = 0;
            continue;
        }
        backslashes += 1;
        fixed_contents.push(char);
    }

    fixed_contents
}
