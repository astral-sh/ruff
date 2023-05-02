use std::iter::Peekable;

use once_cell::sync::Lazy;

use regex::{CaptureMatches, Regex, RegexSet};
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::Tok;
use rustpython_parser::{ast::Location, lexer::LexResult};

use crate::{
    registry::Rule,
    settings::{flags, Settings},
};

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
        format!("Missing issue link following TODO")
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
impl AlwaysAutofixableViolation for InvalidCapitalizationInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidCapitalizationInTodo { tag } = self;
        format!("Invalid TODO capitalization: `{tag}` should be `TODO`")
    }

    fn autofix_title(&self) -> String {
        "Fix capitalization in `TODO`".to_string()
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
//      2. Author exists (in parentheses) with 0 or 1 spaces between it and the tag, but before colon
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
// TODO: tags should be case-insensitive
static TODO_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^# {0,1}(?P<tag>TODO|BUG|FIXME|XXX)( {0,1}\(.*\))?(:)?( )?(.+)?$").unwrap()
});

static ISSUE_LINK_REGEX_SET: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        r#"^#\s*(http|https)://.*"#, // issue link
        r#"^#\s*\d+$"#,              // issue code - like "003"
        r#"^#\s*[A-Z]{1,6}\-?\d+$"#, // issue code - like "TDO-003"
    ])
    .unwrap()
});

static NUM_CAPTURE_GROUPS: usize = 5usize;
static TODO_LENGTH: usize = 4usize;

pub fn check_todos(
    tokens: &[LexResult],
    autofix: flags::Autofix,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let mut iter = tokens.iter().flatten().peekable();
    while let Some((start, token, end)) = iter.next() {
        let Tok::Comment(comment) = token else {
            continue;
        };

        let Some(captures) = get_captured_matches(comment).next() else {
            continue;
        };
        let range = Range::new(*start, *end);

        // Check for errors on the tag: TDO001/TDO006.
        // Unwrap is safe because the "tag" capture group is required to get here.
        let tag = captures.name("tag").unwrap().as_str();
        if tag != "TODO" {
            diagnostics.push(Diagnostic::new(
                InvalidTodoTag {
                    tag: String::from(tag),
                },
                range,
            ));

            if tag.to_uppercase() == "TODO" {
                let mut invalid_capitalization = Diagnostic::new(
                    InvalidCapitalizationInTodo {
                        tag: String::from(tag),
                    },
                    range,
                );

                if autofix.into() && settings.rules.should_fix(Rule::InvalidCapitalizationInTodo) {
                    let first_t_position = find_first_t_position(comment);

                    invalid_capitalization.set_fix(Fix::new(vec![Edit::replacement(
                        "TODO".to_string(),
                        Location::new(range.location.row(), first_t_position),
                        Location::new(range.location.row(), first_t_position + TODO_LENGTH),
                    )]));
                }

                diagnostics.push(invalid_capitalization);
            }
        }

        // Check the rest of the capture groups for errors
        //
        // TODO: improve diagnostic placement - this may involve getting rid of the capture groups.
        // Maybe we iterate through the token and perform each check - this would allow us to find
        // offsets because we could iterate on index, then when you see an error you store the
        // index
        for capture_group_index in 2..=NUM_CAPTURE_GROUPS {
            if captures.get(capture_group_index).is_some() {
                continue;
            }

            if let Some(diagnostic) = get_regex_error(capture_group_index, &range, &mut diagnostics)
            {
                diagnostics.push(diagnostic);
            };
        }

        // If we've gotten all the way here, we know that the current token is a TODO. TDO003
        // requires that the next one is an issue link.
        if let Some((next_start, next_token, next_end)) = iter.peek() {
            if let Tok::Comment(next_comment) = next_token {
                if ISSUE_LINK_REGEX_SET.is_match(next_comment) {
                    continue;
                }
            }

            diagnostics.push(Diagnostic::new(
                MissingLinkInTodo,
                Range::new(*next_start, *next_end),
            ));
        } else {
            // There's a TODO on the last line of the file which can't have a link after it.
            // TODO: restrict the location of the diagnostic to reduce noise
            diagnostics.push(Diagnostic::new(MissingLinkInTodo, range));
        }
    }

    diagnostics
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

fn get_captured_matches(text: &str) -> CaptureMatches {
    TODO_REGEX.captures_iter(text)
}

fn find_first_t_position(comment: &str) -> usize {
    comment.find(['t', 'T']).unwrap()
}
