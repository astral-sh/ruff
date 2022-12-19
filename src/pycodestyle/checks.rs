use itertools::izip;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Located, Location, Stmt, StmtKind};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::source_code_locator::SourceCodeLocator;

static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://\S+$").unwrap());

/// E501
pub fn line_too_long(lineno: usize, line: &str, max_line_length: usize) -> Option<Check> {
    let line_length = line.chars().count();

    if line_length <= max_line_length {
        return None;
    }

    let mut chunks = line.split_whitespace();
    let (Some(first), Some(_)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return None;
    };

    // Do not enforce the line length for commented lines that end with a URL
    // or contain only a single word.
    if first == "#" && chunks.last().map_or(true, |c| URL_REGEX.is_match(c)) {
        return None;
    }

    Some(Check::new(
        CheckKind::LineTooLong(line_length, max_line_length),
        Range {
            location: Location::new(lineno + 1, max_line_length),
            end_location: Location::new(lineno + 1, line_length),
        },
    ))
}

/// E721
pub fn type_comparison(ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    for (op, right) in izip!(ops, comparators) {
        if !matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            continue;
        }
        match &right.node {
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name { id, .. } = &func.node {
                    // Ex) type(False)
                    if id == "type" {
                        if let Some(arg) = args.first() {
                            // Allow comparison for types which are not obvious.
                            if !matches!(arg.node, ExprKind::Name { .. }) {
                                checks.push(Check::new(CheckKind::TypeComparison, location));
                            }
                        }
                    }
                }
            }
            ExprKind::Attribute { value, .. } => {
                if let ExprKind::Name { id, .. } = &value.node {
                    // Ex) types.IntType
                    if id == "types" {
                        checks.push(Check::new(CheckKind::TypeComparison, location));
                    }
                }
            }
            _ => {}
        }
    }

    checks
}

/// E722
pub fn do_not_use_bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    location: Range,
) -> Option<Check> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt.node, StmtKind::Raise { exc: None, .. }))
    {
        Some(Check::new(CheckKind::DoNotUseBareExcept, location))
    } else {
        None
    }
}

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// E741
pub fn ambiguous_variable_name<T>(name: &str, located: &Located<T>) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousVariableName(name.to_string()),
            Range::from_located(located),
        ))
    } else {
        None
    }
}

/// E742
pub fn ambiguous_class_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousClassName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// E743
pub fn ambiguous_function_name(name: &str, location: Range) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousFunctionName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// W292
pub fn no_newline_at_end_of_file(contents: &str) -> Option<Check> {
    if !contents.ends_with('\n') {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = contents.lines().last() {
            return Some(Check::new(
                CheckKind::NoNewLineAtEndOfFile,
                Range {
                    location: Location::new(contents.lines().count(), line.len() + 1),
                    end_location: Location::new(contents.lines().count(), line.len() + 1),
                },
            ));
        }
    }
    None
}

// See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
const VALID_ESCAPE_SEQUENCES: &[char; 23] = &[
    '\n', '\\', '\'', '"', 'a', 'b', 'f', 'n', 'r', 't', 'v', '0', '1', '2', '3', '4', '5', '6',
    '7', 'x', // Escape sequences only recognized in string literals
    'N', 'u', 'U',
];

/// Return the quotation markers used for a String token.
fn extract_quote(text: &str) -> &str {
    for quote in ["'''", "\"\"\"", "'", "\""] {
        if text.ends_with(quote) {
            return quote;
        }
    }

    panic!("Unable to find quotation mark for String token")
}

/// W605
pub fn invalid_escape_sequence(
    locator: &SourceCodeLocator,
    start: Location,
    end: Location,
) -> Vec<Check> {
    let mut checks = vec![];

    let text = locator.slice_source_code_range(&Range {
        location: start,
        end_location: end,
    });

    // Determine whether the string is single- or triple-quoted.
    let quote = extract_quote(&text);
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[(quote_pos + quote.len())..(text.len() - quote.len())];

    if !prefix.contains('r') {
        for (row_offset, line) in body.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for col_offset in 0..chars.len() {
                if chars[col_offset] != '\\' {
                    continue;
                }

                // If the previous character was also a backslash, skip.
                if col_offset > 0 && chars[col_offset - 1] == '\\' {
                    continue;
                }

                // If we're at the end of the line, skip.
                if col_offset == chars.len() - 1 {
                    continue;
                }

                // If the next character is a valid escape sequence, skip.
                let next_char = chars[col_offset + 1];
                if VALID_ESCAPE_SEQUENCES.contains(&next_char) {
                    continue;
                }

                // Compute the location of the escape sequence by offsetting the location of the
                // string token by the characters we've seen thus far.
                let col = if row_offset == 0 {
                    start.column() + prefix.len() + quote.len() + col_offset
                } else {
                    col_offset
                };
                let location = Location::new(start.row() + row_offset, col);
                let end_location = Location::new(location.row(), location.column() + 2);
                checks.push(Check::new(
                    CheckKind::InvalidEscapeSequence(next_char),
                    Range {
                        location,
                        end_location,
                    },
                ));
            }
        }
    }

    checks
}
