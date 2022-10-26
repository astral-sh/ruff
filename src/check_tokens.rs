//! Lint rules based on token traversal.

use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::operations::SourceCodeLocator;
use crate::ast::types::Range;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::Settings;

// See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
const VALID_ESCAPE_SEQUENCES: &[char; 23] = &[
    '\n', '\\', '\'', '"', 'a', 'b', 'f', 'n', 'r', 't', 'v', '0', '1', '2', '3', '4', '5', '6',
    '7', 'x', // Escape sequences only recognized in string literals
    'N', 'u', 'U',
];

/// Return the quotation markers used for a String token.
fn extract_quote(text: &str) -> &str {
    if text.len() >= 3 {
        let triple = &text[text.len() - 3..];
        if triple == "'''" || triple == "\"\"\"" {
            return triple;
        }
    }

    if !text.is_empty() {
        let single = &text[text.len() - 1..];
        if single == "'" || single == "\"" {
            return single;
        }
    }

    panic!("Unable to find quotation mark for String token.")
}

/// W605
fn invalid_escape_sequence(
    locator: &SourceCodeLocator,
    start: &Location,
    end: &Location,
) -> Vec<Check> {
    let mut checks = vec![];

    let text = locator.slice_source_code_range(&Range {
        location: *start,
        end_location: *end,
    });

    // Determine whether the string is single- or triple-quoted.
    let quote = extract_quote(text);
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[(quote_pos + quote.len())..(text.len() - quote.len())];

    if !prefix.contains('r') {
        let mut col_offset = 0;
        let mut row_offset = 0;
        let mut in_escape = false;
        let mut chars = body.chars();
        let mut current = chars.next();
        let mut next = chars.next();
        while let (Some(current_char), Some(next_char)) = (current, next) {
            // If we see an escaped backslash, avoid treating the character _after_ the
            // escaped backslash as itself an escaped character.
            if in_escape {
                in_escape = false;
            } else {
                in_escape = current_char == '\\' && next_char == '\\';
                if current_char == '\\' && !VALID_ESCAPE_SEQUENCES.contains(&next_char) {
                    // Compute the location of the escape sequence by offsetting the location of the
                    // string token by the characters we've seen thus far.
                    let location = if row_offset == 0 {
                        Location::new(
                            start.row() + row_offset,
                            start.column() + prefix.len() + quote.len() + col_offset,
                        )
                    } else {
                        Location::new(start.row() + row_offset, col_offset + 1)
                    };
                    let end_location = Location::new(location.row(), location.column() + 1);
                    checks.push(Check::new(
                        CheckKind::InvalidEscapeSequence(next_char),
                        Range {
                            location,
                            end_location,
                        },
                    ))
                }
            }

            // Track the offset from the start position as we iterate over the body.
            if current_char == '\n' {
                col_offset = 0;
                row_offset += 1;
            } else {
                col_offset += 1;
            }

            current = next;
            next = chars.next();
        }
    }

    checks
}

pub fn check_tokens(
    checks: &mut Vec<Check>,
    contents: &str,
    tokens: &[LexResult],
    settings: &Settings,
) {
    // TODO(charlie): Use a shared SourceCodeLocator between this site and the AST traversal.
    let locator = SourceCodeLocator::new(contents);
    let enforce_invalid_escape_sequence = settings.enabled.contains(&CheckCode::W605);
    for (start, tok, end) in tokens.iter().flatten() {
        if enforce_invalid_escape_sequence {
            if matches!(tok, Tok::String { .. }) {
                checks.extend(invalid_escape_sequence(&locator, start, end));
            }
        }
    }
}
