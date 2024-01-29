use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

use crate::lex::docstring_detection::StateMachine;

use crate::settings::LinterSettings;

use super::super::settings::Quote;

/// ## What it does
/// Checks for inline strings that use single quotes or double quotes,
/// depending on the value of the [`lint.flake8-quotes.inline-quotes`] option.
///
/// ## Why is this bad?
/// Consistency is good. Use either single or double quotes for inline
/// strings, but be consistent.
///
/// ## Example
/// ```python
/// foo = 'bar'
/// ```
///
/// Assuming `inline-quotes` is set to `double`, use instead:
/// ```python
/// foo = "bar"
/// ```
///
/// ## Options
/// - `lint.flake8-quotes.inline-quotes`
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent quotes for inline strings, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct BadQuotesInlineString {
    preferred_quote: Quote,
}

impl AlwaysFixableViolation for BadQuotesInlineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesInlineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => format!("Single quotes found but double quotes preferred"),
            Quote::Single => format!("Double quotes found but single quotes preferred"),
        }
    }

    fn fix_title(&self) -> String {
        let BadQuotesInlineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => "Replace single quotes with double quotes".to_string(),
            Quote::Single => "Replace double quotes with single quotes".to_string(),
        }
    }
}

/// ## What it does
/// Checks for multiline strings that use single quotes or double quotes,
/// depending on the value of the [`lint.flake8-quotes.multiline-quotes`]
/// setting.
///
/// ## Why is this bad?
/// Consistency is good. Use either single or double quotes for multiline
/// strings, but be consistent.
///
/// ## Example
/// ```python
/// foo = '''
/// bar
/// '''
/// ```
///
/// Assuming `multiline-quotes` is set to `double`, use instead:
/// ```python
/// foo = """
/// bar
/// """
/// ```
///
/// ## Options
/// - `lint.flake8-quotes.multiline-quotes`
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces double quotes for multiline strings, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct BadQuotesMultilineString {
    preferred_quote: Quote,
}

impl AlwaysFixableViolation for BadQuotesMultilineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesMultilineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => format!("Single quote multiline found but double quotes preferred"),
            Quote::Single => format!("Double quote multiline found but single quotes preferred"),
        }
    }

    fn fix_title(&self) -> String {
        let BadQuotesMultilineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => "Replace single multiline quotes with double quotes".to_string(),
            Quote::Single => "Replace double multiline quotes with single quotes".to_string(),
        }
    }
}

/// ## What it does
/// Checks for docstrings that use single quotes or double quotes, depending
/// on the value of the [`lint.flake8-quotes.docstring-quotes`] setting.
///
/// ## Why is this bad?
/// Consistency is good. Use either single or double quotes for docstring
/// strings, but be consistent.
///
/// ## Example
/// ```python
/// '''
/// bar
/// '''
/// ```
///
/// Assuming `docstring-quotes` is set to `double`, use instead:
/// ```python
/// """
/// bar
/// """
/// ```
///
/// ## Options
/// - `lint.flake8-quotes.docstring-quotes`
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces double quotes for docstrings, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct BadQuotesDocstring {
    preferred_quote: Quote,
}

impl AlwaysFixableViolation for BadQuotesDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesDocstring { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => format!("Single quote docstring found but double quotes preferred"),
            Quote::Single => format!("Double quote docstring found but single quotes preferred"),
        }
    }

    fn fix_title(&self) -> String {
        let BadQuotesDocstring { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => "Replace single quotes docstring with double quotes".to_string(),
            Quote::Single => "Replace double quotes docstring with single quotes".to_string(),
        }
    }
}

const fn good_multiline(quote: Quote) -> &'static str {
    match quote {
        Quote::Double => "\"\"\"",
        Quote::Single => "'''",
    }
}

const fn good_multiline_ending(quote: Quote) -> &'static str {
    match quote {
        Quote::Double => "\"'''",
        Quote::Single => "'\"\"\"",
    }
}

const fn good_docstring(quote: Quote) -> &'static str {
    match quote {
        Quote::Double => "\"",
        Quote::Single => "'",
    }
}

#[derive(Debug)]
struct Trivia<'a> {
    last_quote_char: char,
    prefix: &'a str,
    raw_text: &'a str,
    is_multiline: bool,
}

impl<'a> From<&'a str> for Trivia<'a> {
    fn from(value: &'a str) -> Self {
        // Remove any prefixes (e.g., remove `u` from `u"foo"`).
        let last_quote_char = value.chars().last().unwrap();
        let first_quote_char = value.find(last_quote_char).unwrap();
        let prefix = &value[..first_quote_char];
        let raw_text = &value[first_quote_char..];

        // Determine if the string is multiline-based.
        let is_multiline = if raw_text.len() >= 3 {
            let mut chars = raw_text.chars();
            let first = chars.next().unwrap();
            let second = chars.next().unwrap();
            let third = chars.next().unwrap();
            first == second && second == third
        } else {
            false
        };

        Self {
            last_quote_char,
            prefix,
            raw_text,
            is_multiline,
        }
    }
}

/// Q002
fn docstring(locator: &Locator, range: TextRange, settings: &LinterSettings) -> Option<Diagnostic> {
    let quotes_settings = &settings.flake8_quotes;

    let text = locator.slice(range);
    let trivia: Trivia = text.into();

    if trivia
        .raw_text
        .contains(good_docstring(quotes_settings.docstring_quotes))
    {
        return None;
    }

    let mut diagnostic = Diagnostic::new(
        BadQuotesDocstring {
            preferred_quote: quotes_settings.docstring_quotes,
        },
        range,
    );
    let quote_count = if trivia.is_multiline { 3 } else { 1 };
    let string_contents = &trivia.raw_text[quote_count..trivia.raw_text.len() - quote_count];
    let quote = good_docstring(quotes_settings.docstring_quotes).repeat(quote_count);
    let mut fixed_contents =
        String::with_capacity(trivia.prefix.len() + string_contents.len() + quote.len() * 2);
    fixed_contents.push_str(trivia.prefix);
    fixed_contents.push_str(&quote);
    fixed_contents.push_str(string_contents);
    fixed_contents.push_str(&quote);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        fixed_contents,
        range,
    )));
    Some(diagnostic)
}

/// Q000, Q001
fn strings(
    locator: &Locator,
    sequence: &[TextRange],
    settings: &LinterSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let quotes_settings = &settings.flake8_quotes;

    let trivia = sequence
        .iter()
        .map(|range| {
            let text = locator.slice(*range);
            let trivia: Trivia = text.into();
            trivia
        })
        .collect::<Vec<_>>();

    // Return `true` if any of the strings are inline strings that contain the quote
    // character in the body.
    let relax_quote = trivia.iter().any(|trivia| {
        if trivia.is_multiline {
            return false;
        }

        if trivia.last_quote_char == quotes_settings.inline_quotes.as_char() {
            return false;
        }

        let string_contents = &trivia.raw_text[1..trivia.raw_text.len() - 1];
        string_contents.contains(quotes_settings.inline_quotes.as_char())
    });

    for (range, trivia) in sequence.iter().zip(trivia) {
        if trivia.is_multiline {
            // If our string is or contains a known good string, ignore it.
            if trivia
                .raw_text
                .contains(good_multiline(quotes_settings.multiline_quotes))
            {
                continue;
            }

            // If our string ends with a known good ending, then ignore it.
            if trivia
                .raw_text
                .ends_with(good_multiline_ending(quotes_settings.multiline_quotes))
            {
                continue;
            }

            let mut diagnostic = Diagnostic::new(
                BadQuotesMultilineString {
                    preferred_quote: quotes_settings.multiline_quotes,
                },
                *range,
            );

            let string_contents = &trivia.raw_text[3..trivia.raw_text.len() - 3];
            let quote = good_multiline(quotes_settings.multiline_quotes);
            let mut fixed_contents = String::with_capacity(
                trivia.prefix.len() + string_contents.len() + quote.len() * 2,
            );
            fixed_contents.push_str(trivia.prefix);
            fixed_contents.push_str(quote);
            fixed_contents.push_str(string_contents);
            fixed_contents.push_str(quote);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                fixed_contents,
                *range,
            )));
            diagnostics.push(diagnostic);
        } else if trivia.last_quote_char != quotes_settings.inline_quotes.as_char()
            // If we're not using the preferred type, only allow use to avoid escapes.
            && !relax_quote
        {
            let mut diagnostic = Diagnostic::new(
                BadQuotesInlineString {
                    preferred_quote: quotes_settings.inline_quotes,
                },
                *range,
            );
            let quote = quotes_settings.inline_quotes.as_char();
            let string_contents = &trivia.raw_text[1..trivia.raw_text.len() - 1];
            let mut fixed_contents =
                String::with_capacity(trivia.prefix.len() + string_contents.len() + 2);
            fixed_contents.push_str(trivia.prefix);
            fixed_contents.push(quote);
            fixed_contents.push_str(string_contents);
            fixed_contents.push(quote);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                fixed_contents,
                *range,
            )));
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

/// A builder for the f-string range.
///
/// For now, this is limited to the outermost f-string and doesn't support
/// nested f-strings.
#[derive(Debug, Default)]
struct FStringRangeBuilder {
    start_location: TextSize,
    end_location: TextSize,
    nesting: u32,
}

impl FStringRangeBuilder {
    fn visit_token(&mut self, token: &Tok, range: TextRange) {
        match token {
            Tok::FStringStart => {
                if self.nesting == 0 {
                    self.start_location = range.start();
                }
                self.nesting += 1;
            }
            Tok::FStringEnd => {
                self.nesting = self.nesting.saturating_sub(1);
                if self.nesting == 0 {
                    self.end_location = range.end();
                }
            }
            _ => {}
        }
    }

    /// Returns `true` if the lexer is currently inside of a f-string.
    ///
    /// It'll return `false` once the `FStringEnd` token for the outermost
    /// f-string is visited.
    const fn in_fstring(&self) -> bool {
        self.nesting > 0
    }

    /// Returns the complete range of the previously visited f-string.
    ///
    /// This method should only be called once the lexer is outside of any
    /// f-string otherwise it might return an invalid range.
    ///
    /// It doesn't consume the builder because there can be multiple f-strings
    /// throughout the source code.
    fn finish(&self) -> TextRange {
        debug_assert!(!self.in_fstring());
        TextRange::new(self.start_location, self.end_location)
    }
}

/// Generate `flake8-quote` diagnostics from a token stream.
pub(crate) fn check_string_quotes(
    diagnostics: &mut Vec<Diagnostic>,
    lxr: &[LexResult],
    locator: &Locator,
    settings: &LinterSettings,
) {
    // Keep track of sequences of strings, which represent implicit string
    // concatenation, and should thus be handled as a single unit.
    let mut sequence = vec![];
    let mut state_machine = StateMachine::default();
    let mut fstring_range_builder = FStringRangeBuilder::default();
    for &(ref tok, range) in lxr.iter().flatten() {
        fstring_range_builder.visit_token(tok, range);
        if fstring_range_builder.in_fstring() {
            continue;
        }

        let is_docstring = state_machine.consume(tok);

        // If this is a docstring, consume the existing sequence, then consume the
        // docstring, then move on.
        if is_docstring {
            if !sequence.is_empty() {
                diagnostics.extend(strings(locator, &sequence, settings));
                sequence.clear();
            }
            if let Some(diagnostic) = docstring(locator, range, settings) {
                diagnostics.push(diagnostic);
            }
        } else {
            match tok {
                Tok::String { .. } => {
                    // If this is a string, add it to the sequence.
                    sequence.push(range);
                }
                Tok::FStringEnd => {
                    // If this is the end of an f-string, add the entire f-string
                    // range to the sequence.
                    sequence.push(fstring_range_builder.finish());
                }
                Tok::Comment(..) | Tok::NonLogicalNewline => continue,
                _ => {
                    // Otherwise, consume the sequence.
                    if !sequence.is_empty() {
                        diagnostics.extend(strings(locator, &sequence, settings));
                        sequence.clear();
                    }
                }
            }
        }
    }

    // If we have an unterminated sequence, consume it.
    if !sequence.is_empty() {
        diagnostics.extend(strings(locator, &sequence, settings));
        sequence.clear();
    }
}
