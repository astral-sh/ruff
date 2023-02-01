use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::lex::docstring_detection::StateMachine;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::Locator;
use crate::violations;

use super::settings::Quote;

fn good_single(quote: &Quote) -> char {
    match quote {
        Quote::Single => '\'',
        Quote::Double => '"',
    }
}

fn bad_single(quote: &Quote) -> char {
    match quote {
        Quote::Double => '\'',
        Quote::Single => '"',
    }
}

fn good_multiline(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'''",
        Quote::Double => "\"\"\"",
    }
}

fn good_multiline_ending(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'\"\"\"",
        Quote::Double => "\"'''",
    }
}

fn good_docstring(quote: &Quote) -> &str {
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
        violations::BadQuotesDocstring {
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

    // Return `true` if any of the strings are inline strings that contain the quote character in
    // the body.
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
                violations::BadQuotesMultilineString {
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
                        Diagnostic::new(violations::AvoidQuoteEscape, Range::new(*start, *end));
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
                    violations::BadQuotesInlineString {
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

    // Keep track of sequences of strings, which represent implicit string concatenation, and
    // should thus be handled as a single unit.
    let mut sequence = vec![];
    let mut state_machine = StateMachine::default();
    for &(start, ref tok, end) in lxr.iter().flatten() {
        let is_docstring = state_machine.consume(tok);

        // If this is a docstring, consume the existing sequence, then consume the docstring, then
        // move on.
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
