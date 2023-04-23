use std::iter::Peekable;

use once_cell::sync::Lazy;

use regex::{CaptureMatches, Regex};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

/// ## What it does
/// Checks that a TODO comment is actually labelled with "TODO".
///
/// ## Why is this bad?
/// Ambiguous tags reduce code visibility and can lead to dangling TODOs.
/// If someone greps for a TODO to fix, but the comment is tagged with a "FIXME"
/// tag instead of a "TODO", that comment may never be found!
///
/// Note that this rule will only flag "FIXME", "BUG", and "XXX" tags as incorrect.
///
/// ## Example
/// ```python
/// # FIXME(ruff): this should get fixed!
/// ```
///
/// Use instead:
/// ```python
/// # TODO(ruff): this is now fixed!
/// ```
#[violation]
pub struct InvalidTodoTag {
    pub tag: String,
}

// TODO - autofix this to just insert TODO instead of the tag?
impl Violation for InvalidTodoTag {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidTodoTag { tag } = self;
        format!("Invalid TODO tag: `{tag}` should be `TODO`")
    }
}

/// ## What it does
/// Checks that a TODO comment has an author assigned to it.
///
/// ## Why is this bad?
/// Assigning an author to a task helps keep it on the radar and keeps code
/// formatting consistent.
///
/// ## Example
/// ```python
/// # TODO: should assign an author here
/// ```
///
/// Use instead
/// ```python
/// # TODO(ruff): now an author is assigned
/// ```
#[violation]
pub struct MissingAuthorInTodo;
impl Violation for MissingAuthorInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing author in TODO")
    }
}

/// ## What it does
/// Checks that an issue link or ticket is associated with a TODO.
///
/// ## Why is this bad?
/// Including an issue link near a TODO makes it easier for resolvers
/// to get context around the issue and keeps code formatting consistent.
///
/// ## Example
/// ```python
/// # TODO: this link has no issue
/// ```
///
/// Use instead:
/// ```python
/// # TODO(ruff): solve this issue!
/// # https://github.com/charliermarsh/ruff/issues/3870
/// ```
#[violation]
pub struct MissingLinkInTodo;
impl Violation for MissingLinkInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing issue link in TODO")
    }
}

/// ## What it does
/// Checks that a "TODO" tag is followed by a colon.
///
/// ## Why is this bad?
/// Skipping colons after a "TODO" tag can create inconsistent code formatting.
///
/// ## Example
/// ```python
/// # TODO(ruff) fix this colon
/// ```
///
/// Used instead:
/// ```python
/// # TODO(ruff): colon fixed
/// ```
#[violation]
pub struct MissingColonInTodo;
impl Violation for MissingColonInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing colon in TODO")
    }
}

/// ## What it does
/// Checks that a "TODO" tag has some text after it.
///
/// ## Why is this bad?
/// Just putting a "TODO" tag in the code, without any context, makes it harder
/// for the reader/future resolver to understand the issue that has to be fixed.
///
/// ## Example
/// ```python
/// # TODO(ruff)
/// ```
///
/// Use instead:
/// ```python
/// # TODO(ruff): fix some issue
/// ```
#[violation]
pub struct MissingTextInTodo;
impl Violation for MissingTextInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing text in TODO")
    }
}

/// ## What it does
/// Checks that a "TODO" tag is properly capitalized, i.e. that the tag is uppercase.
///
/// ## Why is this bad?
/// Inconsistent capitalization leads to less readable code.
///
/// ## Example
/// ```python
/// # todo(ruff): capitalize this
/// ```
///
/// Use instead:
/// ```python
/// # TODO(ruff): this is capitalized
/// ```
#[violation]
pub struct InvalidCapitalizationInTodo {
    pub tag: String,
}
impl Violation for InvalidCapitalizationInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidCapitalizationInTodo { tag } = self;
        format!("Invalid TODO capitalization: `{tag}` should be `TODO`")
    }
}

/// ## What it does
/// Checks that the colon after a "TODO" tag is followed by a space.
///
/// ## Why is this bad?
/// Skipping the space after a colon leads to less readable code.
///
/// ## Example
/// ```python
/// # TODO(ruff):fix this
/// ```
///
/// Use instead:
/// ```python
/// # TODO(ruff): fix this
/// ```
#[violation]
pub struct MissingSpaceAfterColonInTodo;
impl Violation for MissingSpaceAfterColonInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing space after colon in TODO")
    }
}

// Capture groups correspond to checking:
//      1. Tag (used to match against `TODO` in capitalization and spelling, e.g. ToDo and FIXME)
//      2. Author exists (in parentheses) after tag, but before colon
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
// `Nones` for the colon and space checks.
//
// Note: Regexes taken from https://github.com/orsinium-labs/flake8-todos/blob/master/flake8_todos/_rules.py#L12.
static TODO_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^#\s*(?P<tag>[tT][oO][dD][oO]|BUG|FIXME|XXX)( {0,1}\(.*\))?(:)?( )?(.+)?$").unwrap()
});

// Issue code: TDO-003
static ISSUE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^#\s*[A-Z]{1,6}\-?\d+$"#).unwrap());
// Issue code: 003
static TICKET_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^#\s*\d+$"#).unwrap());
// Link to issue
static LINK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^#\s*(http|https)://.*"#).unwrap());
static NUM_CAPTURE_GROUPS: usize = 5usize;

pub fn check_rules(tokens: &[LexResult]) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];
    let mut prev_token_is_todo = false;

    for (start, token, end) in tokens.iter().flatten() {
        let Tok::Comment(comment) = token else {
            continue;
        };

        let diagnostics_ref = &mut diagnostics;
        let range = Range::new(*start, *end);

        // The previous token is a TODO, so let's check if this token is a link
        if prev_token_is_todo {
            if ISSUE_REGEX.is_match(comment)
                || TICKET_REGEX.is_match(comment)
                || LINK_REGEX.is_match(comment)
            {
                prev_token_is_todo = false;
                // continuing here avoids an expensive call to captures_iter() - we know that this
                // line, if it's a link, can't be another TODO
                continue;
            }

            diagnostics_ref.push(Diagnostic::new(MissingLinkInTodo, range));
        }

        if let Some(captures_ref) = get_captured_matches(comment).peek() {
            let captures = captures_ref.to_owned();

            // captures.get(1) is the tag, which is required for the regex to match. The unwrap()
            // call is therefore safe
            let tag = captures.name("tag").unwrap().as_str();
            if tag != "TODO" {
                diagnostics_ref.push(Diagnostic::new(
                    InvalidTodoTag {
                        tag: String::from(tag),
                    },
                    range,
                ));

                if tag.to_uppercase() == "TODO" {
                    diagnostics_ref.push(Diagnostic::new(
                        InvalidCapitalizationInTodo {
                            tag: String::from(tag),
                        },
                        range,
                    ));
                }
            }

            for capture_group_index in 2..=NUM_CAPTURE_GROUPS {
                if captures.get(capture_group_index).is_some() {
                    continue;
                }

                if let Some(diagnostic) =
                    get_regex_error(capture_group_index, &range, diagnostics_ref)
                {
                    diagnostics_ref.push(diagnostic);
                };
            }

            prev_token_is_todo = true;
        } else {
            prev_token_is_todo = false;
        }
    }

    diagnostics
}

fn get_captured_matches(text: &str) -> Peekable<CaptureMatches> {
    TODO_REGEX.captures_iter(text).peekable()
}

/// Mapper for static regex errors caused by a capture group at index i (i > 1 since the tag
/// capture group could lead to multiple diagnostics being pushed)
fn get_regex_error(i: usize, range: &Range, diagnostics: &mut [Diagnostic]) -> Option<Diagnostic> {
    match i {
        2usize => Some(Diagnostic::new(MissingAuthorInTodo, *range)),
        3usize => Some(Diagnostic::new(MissingColonInTodo, *range)),
        4usize => {
            if diagnostics
                .last()
                .map_or(true, |last| last.kind != MissingColonInTodo.into())
            {
                Some(Diagnostic::new(MissingSpaceAfterColonInTodo, *range))
            } else {
                None
            }
        }
        5usize => Some(Diagnostic::new(MissingTextInTodo, *range)),
        _ => None,
    }
}
