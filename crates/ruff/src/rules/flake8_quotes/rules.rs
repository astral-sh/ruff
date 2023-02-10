use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use super::settings::Quote;
use crate::ast::types::Range;
use crate::fix::Fix;
use crate::lex::docstring_detection::StateMachine;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ### What it does
    /// Checks for inline strings that use single quotes or double quotes,
    /// depending on the value of the [`inline-quotes`](https://github.com/charliermarsh/ruff#inline-quotes)
    /// setting.
    ///
    /// ### Why is this bad?
    /// Consistency is good. Use either single or double quotes for inline
    /// strings, but be consistent.
    ///
    /// ### Example
    /// ```python
    /// foo = 'bar'
    /// ```
    ///
    /// Assuming `inline-quotes` is set to `double`, use instead:
    /// ```python
    /// foo = "bar"
    /// ```
    pub struct BadQuotesInlineString {
        pub quote: Quote,
    }
);
impl AlwaysAutofixableViolation for BadQuotesInlineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesInlineString { quote } = self;
        match quote {
            Quote::Single => format!("Double quotes found but single quotes preferred"),
            Quote::Double => format!("Single quotes found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesInlineString { quote } = self;
        match quote {
            Quote::Single => "Replace double quotes with single quotes".to_string(),
            Quote::Double => "Replace single quotes with double quotes".to_string(),
        }
    }
}

define_violation!(
    /// ### What it does
    /// Checks for multiline strings that use single quotes or double quotes,
    /// depending on the value of the [`multiline-quotes`](https://github.com/charliermarsh/ruff#multiline-quotes)
    /// setting.
    ///
    /// ### Why is this bad?
    /// Consistency is good. Use either single or double quotes for multiline
    /// strings, but be consistent.
    ///
    /// ### Example
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
    pub struct BadQuotesMultilineString {
        pub quote: Quote,
    }
);
impl AlwaysAutofixableViolation for BadQuotesMultilineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesMultilineString { quote } = self;
        match quote {
            Quote::Single => format!("Double quote multiline found but single quotes preferred"),
            Quote::Double => format!("Single quote multiline found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesMultilineString { quote } = self;
        match quote {
            Quote::Single => "Replace double multiline quotes with single quotes".to_string(),
            Quote::Double => "Replace single multiline quotes with double quotes".to_string(),
        }
    }
}

define_violation!(
    /// ### What it does
    /// Checks for docstrings that use single quotes or double quotes, depending on the value of the [`docstring-quotes`](https://github.com/charliermarsh/ruff#docstring-quotes)
    /// setting.
    ///
    /// ### Why is this bad?
    /// Consistency is good. Use either single or double quotes for docstring
    /// strings, but be consistent.
    ///
    /// ### Example
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
    pub struct BadQuotesDocstring {
        pub quote: Quote,
    }
);
impl AlwaysAutofixableViolation for BadQuotesDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesDocstring { quote } = self;
        match quote {
            Quote::Single => format!("Double quote docstring found but single quotes preferred"),
            Quote::Double => format!("Single quote docstring found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesDocstring { quote } = self;
        match quote {
            Quote::Single => "Replace double quotes docstring with single quotes".to_string(),
            Quote::Double => "Replace single quotes docstring with double quotes".to_string(),
        }
    }
}

define_violation!(
    /// ### What it does
    /// Checks for strings that include escaped quotes, and suggests changing
    /// the quote style to avoid the need to escape them.
    ///
    /// ### Why is this bad?
    /// It's preferable to avoid escaped quotes in strings. By changing the
    /// outer quote style, you can avoid escaping inner quotes.
    ///
    /// ### Example
    /// ```python
    /// foo = 'bar\'s'
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// foo = "bar's"
    /// ```
    pub struct AvoidQuoteEscape;
);
impl AlwaysAutofixableViolation for AvoidQuoteEscape {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Change outer quotes to avoid escaping inner quotes")
    }

    fn autofix_title(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }
}

const fn good_single(quote: &Quote) -> char {
    match quote {
        Quote::Single => '\'',
        Quote::Double => '"',
    }
}

const fn bad_single(quote: &Quote) -> char {
    match quote {
        Quote::Double => '\'',
        Quote::Single => '"',
    }
}

const fn good_multiline(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'''",
        Quote::Double => "\"\"\"",
    }
}

const fn good_multiline_ending(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'\"\"\"",
        Quote::Double => "\"'''",
    }
}

const fn good_docstring(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'",
        Quote::Double => "\"",
    }
}

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

/// Q003
fn docstring(
    locator: &Locator,
    start: Location,
    end: Location,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    let quotes_settings = &settings.flake8_quotes;

    let text = locator.slice_source_code_range(&Range::new(start, end));
    let trivia: Trivia = text.into();

    if trivia
        .raw_text
        .contains(good_docstring(&quotes_settings.docstring_quotes))
    {
        return None;
    }

    let mut diagnostic = Diagnostic::new(
        BadQuotesDocstring {
            quote: quotes_settings.docstring_quotes.clone(),
        },
        Range::new(start, end),
    );
    if matches!(autofix, flags::Autofix::Enabled)
        && settings.rules.should_fix(&Rule::BadQuotesDocstring)
    {
        let quote_count = if trivia.is_multiline { 3 } else { 1 };
        let string_contents = &trivia.raw_text[quote_count..trivia.raw_text.len() - quote_count];
        let quote = good_docstring(&quotes_settings.docstring_quotes).repeat(quote_count);
        let mut fixed_contents =
            String::with_capacity(trivia.prefix.len() + string_contents.len() + quote.len() * 2);
        fixed_contents.push_str(trivia.prefix);
        fixed_contents.push_str(&quote);
        fixed_contents.push_str(string_contents);
        fixed_contents.push_str(&quote);
        diagnostic.amend(Fix::replacement(fixed_contents, start, end));
    }
    Some(diagnostic)
}

/// Q001, Q002
fn strings(
    locator: &Locator,
    sequence: &[(Location, Location)],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let quotes_settings = &settings.flake8_quotes;

    let trivia = sequence
        .iter()
        .map(|(start, end)| {
            let text = locator.slice_source_code_range(&Range::new(*start, *end));
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

        if trivia.last_quote_char == good_single(&quotes_settings.inline_quotes) {
            return false;
        }

        let string_contents = &trivia.raw_text[1..trivia.raw_text.len() - 1];
        string_contents.contains(good_single(&quotes_settings.inline_quotes))
    });

    for ((start, end), trivia) in sequence.iter().zip(trivia.into_iter()) {
        if trivia.is_multiline {
            // If our string is or contains a known good string, ignore it.
            if trivia
                .raw_text
                .contains(good_multiline(&quotes_settings.multiline_quotes))
            {
                continue;
            }

            // If our string ends with a known good ending, then ignore it.
            if trivia
                .raw_text
                .ends_with(good_multiline_ending(&quotes_settings.multiline_quotes))
            {
                continue;
            }

            let mut diagnostic = Diagnostic::new(
                BadQuotesMultilineString {
                    quote: quotes_settings.multiline_quotes.clone(),
                },
                Range::new(*start, *end),
            );

            if matches!(autofix, flags::Autofix::Enabled)
                && settings.rules.should_fix(&Rule::BadQuotesMultilineString)
            {
                let string_contents = &trivia.raw_text[3..trivia.raw_text.len() - 3];
                let quote = good_multiline(&quotes_settings.multiline_quotes);
                let mut fixed_contents = String::with_capacity(
                    trivia.prefix.len() + string_contents.len() + quote.len() * 2,
                );
                fixed_contents.push_str(trivia.prefix);
                fixed_contents.push_str(quote);
                fixed_contents.push_str(string_contents);
                fixed_contents.push_str(quote);
                diagnostic.amend(Fix::replacement(fixed_contents, *start, *end));
            }
            diagnostics.push(diagnostic);
        } else {
            let string_contents = &trivia.raw_text[1..trivia.raw_text.len() - 1];

            // If we're using the preferred quotation type, check for escapes.
            if trivia.last_quote_char == good_single(&quotes_settings.inline_quotes) {
                if !quotes_settings.avoid_escape
                    || trivia.prefix.contains('r')
                    || trivia.prefix.contains('R')
                {
                    continue;
                }

                if string_contents.contains(good_single(&quotes_settings.inline_quotes))
                    && !string_contents.contains(bad_single(&quotes_settings.inline_quotes))
                {
                    let mut diagnostic =
                        Diagnostic::new(AvoidQuoteEscape, Range::new(*start, *end));
                    if matches!(autofix, flags::Autofix::Enabled)
                        && settings.rules.should_fix(&Rule::AvoidQuoteEscape)
                    {
                        let quote = bad_single(&quotes_settings.inline_quotes);

                        let mut fixed_contents =
                            String::with_capacity(trivia.prefix.len() + string_contents.len() + 2);
                        fixed_contents.push_str(trivia.prefix);
                        fixed_contents.push(quote);

                        let chars: Vec<char> = string_contents.chars().collect();
                        let mut backslash_count = 0;
                        for col_offset in 0..chars.len() {
                            let char = chars[col_offset];
                            if char != '\\' {
                                fixed_contents.push(char);
                                continue;
                            }
                            backslash_count += 1;
                            // If the previous character was also a backslash
                            if col_offset > 0
                                && chars[col_offset - 1] == '\\'
                                && backslash_count == 2
                            {
                                fixed_contents.push(char);
                                // reset to 0
                                backslash_count = 0;
                                continue;
                            }
                            // If we're at the end of the line
                            if col_offset == chars.len() - 1 {
                                fixed_contents.push(char);
                                continue;
                            }
                            let next_char = chars[col_offset + 1];
                            // Remove quote escape
                            if next_char == '\'' || next_char == '"' {
                                // reset to 0
                                backslash_count = 0;
                                continue;
                            }
                            fixed_contents.push(char);
                        }

                        fixed_contents.push(quote);

                        diagnostic.amend(Fix::replacement(fixed_contents, *start, *end));
                    }
                    diagnostics.push(diagnostic);
                }
                continue;
            }

            // If we're not using the preferred type, only allow use to avoid escapes.
            if !relax_quote {
                let mut diagnostic = Diagnostic::new(
                    BadQuotesInlineString {
                        quote: quotes_settings.inline_quotes.clone(),
                    },
                    Range::new(*start, *end),
                );
                if matches!(autofix, flags::Autofix::Enabled)
                    && settings.rules.should_fix(&Rule::BadQuotesInlineString)
                {
                    let quote = good_single(&quotes_settings.inline_quotes);
                    let mut fixed_contents =
                        String::with_capacity(trivia.prefix.len() + string_contents.len() + 2);
                    fixed_contents.push_str(trivia.prefix);
                    fixed_contents.push(quote);
                    fixed_contents.push_str(string_contents);
                    fixed_contents.push(quote);
                    diagnostic.amend(Fix::replacement(fixed_contents, *start, *end));
                }
                diagnostics.push(diagnostic);
            }
        }
    }

    diagnostics
}

/// Generate `flake8-quote` diagnostics from a token stream.
pub fn from_tokens(
    lxr: &[LexResult],
    locator: &Locator,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    // Keep track of sequences of strings, which represent implicit string
    // concatenation, and should thus be handled as a single unit.
    let mut sequence = vec![];
    let mut state_machine = StateMachine::default();
    for &(start, ref tok, end) in lxr.iter().flatten() {
        let is_docstring = state_machine.consume(tok);

        // If this is a docstring, consume the existing sequence, then consume the
        // docstring, then move on.
        if is_docstring {
            if !sequence.is_empty() {
                diagnostics.extend(strings(locator, &sequence, settings, autofix));
                sequence.clear();
            }
            if let Some(diagnostic) = docstring(locator, start, end, settings, autofix) {
                diagnostics.push(diagnostic);
            }
        } else {
            if matches!(tok, Tok::String { .. }) {
                // If this is a string, add it to the sequence.
                sequence.push((start, end));
            } else if !matches!(tok, Tok::Comment(..) | Tok::NonLogicalNewline) {
                // Otherwise, consume the sequence.
                if !sequence.is_empty() {
                    diagnostics.extend(strings(locator, &sequence, settings, autofix));
                    sequence.clear();
                }
            }
        }
    }

    // If we have an unterminated sequence, consume it.
    if !sequence.is_empty() {
        diagnostics.extend(strings(locator, &sequence, settings, autofix));
        sequence.clear();
    }

    diagnostics
}
