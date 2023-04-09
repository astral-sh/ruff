use std::iter::Peekable;

use once_cell::sync::Lazy;

use regex::{CaptureMatches, Regex};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

#[violation]
pub struct InvalidTODOTag {
    pub tag: String,
}

// TODO - autofix this to just insert TODO instead of the tag?
impl Violation for InvalidTODOTag {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidTODOTag { tag } = self;
        format!("Invalid TODO tag: `{tag}` should be `TODO`")
    }
}

#[violation]
pub struct TODOMissingAuthor;
impl Violation for TODOMissingAuthor {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing author into TODO")
    }
}

#[violation]
pub struct MissingLink;
impl Violation for MissingLink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("To be implemented")
    }
}

#[violation]
pub struct TODOMissingColon;
impl Violation for TODOMissingColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing colon in TODO")
    }
}
#[violation]
pub struct TODOMissingText;
impl Violation for TODOMissingText {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing text in TODO")
    }
}

// TODO
// #[violation]
// pub struct InvalidTODOCapitalization {
//     pub tag: String,
// }

#[violation]
pub struct TODOMissingSpaceAfterColon;
impl Violation for TODOMissingSpaceAfterColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing space after colon in TODO")
    }
}

// Capture groups correspond to checking:
//      1. Tag (used to match against `TODO` in capitalization and spelling, e.g. ToDo and FIXME)
//      2. Author exists (in parenthese) after tag, but before colon
//      3. Colon exists after author
//      4. Space exists after colon
//      5. Text exists after space
//
// We can check if any of these exist in one regex. Capture groups that don't pick anything up
// evaluate to `None` in Rust, so the capture group index will always correspond to its respective rule
// whether the token has been found or not.
//
// Example:
// ```python
// # TODO(evanrittenhouse): This is a completely valid TODO
// ```
// will yield [Some("TODO") Some("evanrittenhouse"), Some(":"), Some(" "), Some("This is a completely valid TODO")], whereas
// ```python
// # ToDo this is completely wrong
// ```
// will yield [Some("ToDo"), None, None, Some(" "), Some("this is completely wrong")]. Note the
// `Nones` for the colon and space checks
//
// Note: Tags taken from https://github.com/orsinium-labs/flake8-todos/blob/master/flake8_todos/_rules.py#L12.
static TODO_REGEX: Lazy<Regex> = Lazy::new(|| {
    // TODO BEFORE COMMITTING - <space> should be a nested group inside of <colon>
    Regex::new(r"^#\s*(TODO|BUG|FIXME|XXX)(\(.*\))?(:)?( )?(.+)?$").unwrap()
});
static NUM_CAPTURE_GROUPS: usize = 5usize;

pub fn check_rules(tokens: &[LexResult]) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    for (start, token, end) in tokens.iter().flatten() {
        let Tok::Comment(comment) = token else {
            continue;
        };

        if get_captured_matches(comment).peek().is_some() {
            diagnostics.extend(get_tag_regex_errors(comment, start, end));
        }
    }

    diagnostics
}

fn get_captured_matches(text: &String) -> Peekable<CaptureMatches> {
    TODO_REGEX.captures_iter(text).peekable()
}

fn get_tag_regex_errors(text: &String, start: &Location, end: &Location) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    for capture in TODO_REGEX.captures_iter(text) {
        // The tag is required for capturing the regex, so this is safe.
        let tag = capture.get(1).unwrap().as_str();
        if tag != "TODO" {
            diagnostics.push(Diagnostic::new(
                InvalidTODOTag {
                    tag: String::from(tag),
                },
                Range::new(*start, *end),
            ));

            // TODO: T006 check can go here
        }

        // Note: This initially looks bad from a speed perspective, but is O(1) given that we
        // know that there will only ever be 1 `capture` (due to regex anchors) and constant
        // capture groups.
        for capture_group_index in 2..NUM_CAPTURE_GROUPS + 1 {
            if capture.get(capture_group_index).is_none() {
                let range = Range::new(*start, *end);
                diagnostics.push(match capture_group_index {
                    2usize => Diagnostic::new(TODOMissingAuthor, range),
                    3usize => Diagnostic::new(TODOMissingColon, range),
                    4usize => {
                        if diagnostics
                            .last()
                            .map_or(true, |last| last.kind != TODOMissingColon.into())
                        {
                            Diagnostic::new(TODOMissingSpaceAfterColon, range)
                        } else {
                            continue;
                        }
                    }
                    5usize => Diagnostic::new(TODOMissingText, range),
                    _ => break,
                });
            }
        }
    }

    diagnostics
}
