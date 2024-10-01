use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::StringLike;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

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

impl Violation for BadQuotesInlineString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesInlineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => format!("Single quotes found but double quotes preferred"),
            Quote::Single => format!("Double quotes found but single quotes preferred"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let BadQuotesInlineString { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => Some("Replace single quotes with double quotes".to_string()),
            Quote::Single => Some("Replace double quotes with single quotes".to_string()),
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

impl Violation for BadQuotesDocstring {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesDocstring { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => format!("Single quote docstring found but double quotes preferred"),
            Quote::Single => format!("Double quote docstring found but single quotes preferred"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let BadQuotesDocstring { preferred_quote } = self;
        match preferred_quote {
            Quote::Double => Some("Replace single quotes docstring with double quotes".to_string()),
            Quote::Single => Some("Replace double quotes docstring with single quotes".to_string()),
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

const fn good_docstring(quote: Quote) -> char {
    match quote {
        Quote::Double => '"',
        Quote::Single => '\'',
    }
}

#[derive(Debug)]
struct Trivia<'a> {
    last_quote_char: char,
    prefix: &'a str,
    raw_text: &'a str,
    is_multiline: bool,
}

impl Trivia<'_> {
    fn has_empty_text(&self) -> bool {
        self.raw_text == "\"\"" || self.raw_text == "''"
    }
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

/// Returns `true` if the [`TextRange`] is preceded by two consecutive quotes.
fn text_starts_at_consecutive_quote(locator: &Locator, range: TextRange, quote: Quote) -> bool {
    let mut previous_two_chars = locator.up_to(range.start()).chars().rev();
    previous_two_chars.next() == Some(good_docstring(quote))
        && previous_two_chars.next() == Some(good_docstring(quote))
}

/// Returns `true` if the [`TextRange`] ends at a quote character.
fn text_ends_at_quote(locator: &Locator, range: TextRange, quote: Quote) -> bool {
    locator
        .after(range.end())
        .starts_with(good_docstring(quote))
}

/// Q002
fn docstring(checker: &mut Checker, range: TextRange) {
    let quotes_settings = &checker.settings.flake8_quotes;
    let locator = checker.locator();

    let text = locator.slice(range);
    let trivia: Trivia = text.into();
    if trivia.has_empty_text()
        && text_ends_at_quote(locator, range, quotes_settings.docstring_quotes)
    {
        // Fixing this would result in a one-sided multi-line docstring, which would
        // introduce a syntax error.
        checker.diagnostics.push(Diagnostic::new(
            BadQuotesDocstring {
                preferred_quote: quotes_settings.docstring_quotes,
            },
            range,
        ));
        return;
    }

    if trivia
        .raw_text
        .contains(good_docstring(quotes_settings.docstring_quotes))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        BadQuotesDocstring {
            preferred_quote: quotes_settings.docstring_quotes,
        },
        range,
    );
    let quote_count = if trivia.is_multiline { 3 } else { 1 };
    let string_contents = &trivia.raw_text[quote_count..trivia.raw_text.len() - quote_count];
    let quote = good_docstring(quotes_settings.docstring_quotes)
        .to_string()
        .repeat(quote_count);
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
    checker.diagnostics.push(diagnostic);
}

/// Q000, Q001
fn strings(checker: &mut Checker, sequence: &[TextRange]) {
    let quotes_settings = &checker.settings.flake8_quotes;
    let locator = checker.locator();

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
            // If multiline strings aren't enforced, ignore it.
            if !checker.enabled(Rule::BadQuotesMultilineString) {
                continue;
            }

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
            checker.diagnostics.push(diagnostic);
        } else if trivia.last_quote_char != quotes_settings.inline_quotes.as_char()
            // If we're not using the preferred type, only allow use to avoid escapes.
            && !relax_quote
        {
            // If inline strings aren't enforced, ignore it.
            if !checker.enabled(Rule::BadQuotesInlineString) {
                continue;
            }

            if trivia.has_empty_text()
                && text_ends_at_quote(locator, *range, quotes_settings.inline_quotes)
            {
                // Fixing this would introduce a syntax error. For example, changing the initial
                // single quotes to double quotes would result in a syntax error:
                // ```python
                // ''"assert" ' SAM macro definitions '''
                // ```
                checker.diagnostics.push(Diagnostic::new(
                    BadQuotesInlineString {
                        preferred_quote: quotes_settings.inline_quotes,
                    },
                    *range,
                ));
                continue;
            }

            if text_starts_at_consecutive_quote(locator, *range, quotes_settings.inline_quotes) {
                // Fixing this would introduce a syntax error. For example, changing the double
                // doubles to single quotes would result in a syntax error:
                // ```python
                // ''"assert" ' SAM macro definitions '''
                // ```
                checker.diagnostics.push(Diagnostic::new(
                    BadQuotesInlineString {
                        preferred_quote: quotes_settings.inline_quotes,
                    },
                    *range,
                ));
                continue;
            }

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
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Generate `flake8-quote` diagnostics from a token stream.
pub(crate) fn check_string_quotes(checker: &mut Checker, string_like: StringLike) {
    // Ignore if the string is part of a forward reference. For example,
    // `x: "Literal['foo', 'bar']"`.
    if checker.semantic().in_string_type_definition() {
        return;
    }

    // TODO(dhruvmanila): Support checking for escaped quotes in f-strings.
    if checker.semantic().in_f_string_replacement_field() {
        return;
    }

    let ranges: Vec<_> = string_like.parts().map(|part| part.range()).collect();

    if checker.semantic().in_pep_257_docstring() {
        if checker.enabled(Rule::BadQuotesDocstring) {
            for range in ranges {
                docstring(checker, range);
            }
        }
    } else {
        if checker.any_enabled(&[Rule::BadQuotesInlineString, Rule::BadQuotesMultilineString]) {
            strings(checker, &ranges);
        }
    }
}
