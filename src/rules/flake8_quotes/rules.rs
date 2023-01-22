use rustpython_ast::Location;

use super::settings::Quote;
use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::Locator;
use crate::violations;

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

pub fn quotes(
    locator: &Locator,
    start: Location,
    end: Location,
    is_docstring: bool,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    let quotes_settings = &settings.flake8_quotes;
    let text = locator.slice_source_code_range(&Range::new(start, end));

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
        if raw_text.contains(good_docstring(&quotes_settings.docstring_quotes)) {
            return None;
        }

        let mut diagnostic = Diagnostic::new(
            violations::BadQuotesDocstring(quotes_settings.docstring_quotes.clone()),
            Range::new(start, end),
        );
        if matches!(autofix, flags::Autofix::Enabled)
            && settings.rules.should_fix(&Rule::BadQuotesDocstring)
        {
            let quote_count = if is_multiline { 3 } else { 1 };
            let string_contents = &raw_text[quote_count..raw_text.len() - quote_count];
            let quote = good_docstring(&quotes_settings.docstring_quotes).repeat(quote_count);
            let mut fixed_contents =
                String::with_capacity(prefix.len() + string_contents.len() + quote.len() * 2);
            fixed_contents.push_str(prefix);
            fixed_contents.push_str(&quote);
            fixed_contents.push_str(string_contents);
            fixed_contents.push_str(&quote);
            diagnostic.amend(Fix::replacement(fixed_contents, start, end));
        }
        Some(diagnostic)
    } else if is_multiline {
        // If our string is or contains a known good string, ignore it.
        if raw_text.contains(good_multiline(&quotes_settings.multiline_quotes)) {
            return None;
        }

        // If our string ends with a known good ending, then ignore it.
        if raw_text.ends_with(good_multiline_ending(&quotes_settings.multiline_quotes)) {
            return None;
        }

        let mut diagnostic = Diagnostic::new(
            violations::BadQuotesMultilineString(quotes_settings.multiline_quotes.clone()),
            Range::new(start, end),
        );

        if matches!(autofix, flags::Autofix::Enabled)
            && settings.rules.should_fix(&Rule::BadQuotesMultilineString)
        {
            let string_contents = &raw_text[3..raw_text.len() - 3];
            let quote = good_multiline(&quotes_settings.multiline_quotes);
            let mut fixed_contents =
                String::with_capacity(prefix.len() + string_contents.len() + quote.len() * 2);
            fixed_contents.push_str(prefix);
            fixed_contents.push_str(quote);
            fixed_contents.push_str(string_contents);
            fixed_contents.push_str(quote);
            diagnostic.amend(Fix::replacement(fixed_contents, start, end));
        }
        Some(diagnostic)
    } else {
        let string_contents = &raw_text[1..raw_text.len() - 1];

        // If we're using the preferred quotation type, check for escapes.
        if last_quote_char == good_single(&quotes_settings.inline_quotes) {
            if !quotes_settings.avoid_escape || prefix.contains('r') {
                return None;
            }
            if string_contents.contains(good_single(&quotes_settings.inline_quotes))
                && !string_contents.contains(bad_single(&quotes_settings.inline_quotes))
            {
                let mut diagnostic =
                    Diagnostic::new(violations::AvoidQuoteEscape, Range::new(start, end));
                if matches!(autofix, flags::Autofix::Enabled)
                    && settings.rules.should_fix(&Rule::AvoidQuoteEscape)
                {
                    let quote = bad_single(&quotes_settings.inline_quotes);

                    let mut fixed_contents =
                        String::with_capacity(prefix.len() + string_contents.len() + 2);
                    fixed_contents.push_str(prefix);
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
                        if col_offset > 0 && chars[col_offset - 1] == '\\' && backslash_count == 2 {
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

                    diagnostic.amend(Fix::replacement(fixed_contents, start, end));
                }
                return Some(diagnostic);
            }
            return None;
        }

        // If we're not using the preferred type, only allow use to avoid escapes.
        if !string_contents.contains(good_single(&quotes_settings.inline_quotes)) {
            let mut diagnostic = Diagnostic::new(
                violations::BadQuotesInlineString(quotes_settings.inline_quotes.clone()),
                Range::new(start, end),
            );
            if matches!(autofix, flags::Autofix::Enabled)
                && settings.rules.should_fix(&Rule::BadQuotesInlineString)
            {
                let quote = good_single(&quotes_settings.inline_quotes);
                let mut fixed_contents =
                    String::with_capacity(prefix.len() + string_contents.len() + 2);
                fixed_contents.push_str(prefix);
                fixed_contents.push(quote);
                fixed_contents.push_str(string_contents);
                fixed_contents.push(quote);
                diagnostic.amend(Fix::replacement(fixed_contents, start, end));
            }
            return Some(diagnostic);
        }

        None
    }
}
