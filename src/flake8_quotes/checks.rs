use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_quotes::settings::{Quote, Settings};
use crate::source_code_locator::SourceCodeLocator;

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
        Quote::Single => "'''",
        Quote::Double => "\"\"\"",
    }
}

pub fn quotes(
    locator: &SourceCodeLocator,
    start: &Location,
    end: &Location,
    is_docstring: bool,
    settings: &Settings,
) -> Option<Check> {
    let text = locator.slice_source_code_range(&Range {
        location: *start,
        end_location: *end,
    });

    // Remove any prefixes (e.g., remove `u` from `u"foo"`).
    let last_quote_char = text.chars().last().unwrap();
    let first_quote_char = text.find(last_quote_char).unwrap();
    let prefix = &text[..first_quote_char].to_lowercase();
    let raw_text = &text[first_quote_char..];

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

    if is_docstring {
        if raw_text.contains(good_docstring(&settings.docstring_quotes)) {
            return None;
        }

        return Some(Check::new(
            CheckKind::BadQuotesDocstring(settings.docstring_quotes.clone()),
            Range {
                location: *start,
                end_location: *end,
            },
        ));
    } else if is_multiline {
        // If our string is or contains a known good string, ignore it.
        if raw_text.contains(good_multiline(&settings.multiline_quotes)) {
            return None;
        }

        // If our string ends with a known good ending, then ignore it.
        if raw_text.ends_with(good_multiline_ending(&settings.multiline_quotes)) {
            return None;
        }

        return Some(Check::new(
            CheckKind::BadQuotesMultilineString(settings.multiline_quotes.clone()),
            Range {
                location: *start,
                end_location: *end,
            },
        ));
    } else {
        let string_contents = &raw_text[1..raw_text.len() - 1];

        // If we're using the preferred quotation type, check for escapes.
        if last_quote_char == good_single(&settings.inline_quotes) {
            if !settings.avoid_escape || prefix.contains('r') {
                return None;
            }
            if string_contents.contains(good_single(&settings.inline_quotes))
                && !string_contents.contains(bad_single(&settings.inline_quotes))
            {
                return Some(Check::new(
                    CheckKind::AvoidQuoteEscape,
                    Range {
                        location: *start,
                        end_location: *end,
                    },
                ));
            }
            return None;
        }

        // If we're not using the preferred type, only allow use to avoid escapes.
        if !string_contents.contains(good_single(&settings.inline_quotes)) {
            return Some(Check::new(
                CheckKind::BadQuotesInlineString(settings.inline_quotes.clone()),
                Range {
                    location: *start,
                    end_location: *end,
                },
            ));
        }
    }

    None
}
