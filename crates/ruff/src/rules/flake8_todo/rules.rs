use std::{collections::HashMap, str::Chars};

use once_cell::sync::Lazy;

use regex::{CaptureMatches, Regex, RegexSet};
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

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
        format!("Missing author in TODO. Try: # TODO (<author_name>): ...")
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
/// Use one of these instead:
/// ```python
/// # TODO (ruff): this comment has an issue link
/// # https://github.com/charliermarsh/ruff/issues/3870
///
/// # TODO (ruff): this comment has a 3-digit issue code
/// # 003
///
/// # TODO (ruff): this comment has an issue code of (up to) 6 characters, then digits
/// # SIXCHR-003
/// ```
#[violation]
pub struct MissingLinkInTodo;
impl Violation for MissingLinkInTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing issue link on the line following this TODO")
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
        format!("Missing colon in TODO. Try: # TODO: ...")
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
        format!("Missing text after 'TODO'")
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
        let InvalidCapitalizationInTodo { tag } = self;
        format!("Replace `{tag}` with `TODO`")
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
static TODO_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^# {0,1}(?i)(?P<tag>TODO|BUG|FIXME|XXX)(?-i)( {0,1}\(.*\))?(:)?( )?(.+)?$")
        .unwrap()
});

// Matches against any of the 4 recognized PATTERNS.
static TODO_REGEX_SET: Lazy<RegexSet> = Lazy::new(|| {
    let patterns: [&str; 4] = [
        r#"^#\s*(?i)(TODO).*$"#,
        r#"^#\s*(?i)(BUG).*$"#,
        r#"^#\s*(?i)(FIXME).*$"#,
        r#"^#\s*(?i)(XXX).*$"#,
    ];

    return RegexSet::new(patterns).unwrap();
});

// Maps the index of a particular Regex (specified by its index in the above PATTERNS slice) to the length of the
// tag that we're trying to capture.
static PATTERN_TAG_LENGTH: Lazy<HashMap<usize, usize>> = Lazy::new(|| {
    HashMap::from([
        (0usize, 4usize),
        (1usize, 3usize),
        (2usize, 5usize),
        (3usize, 3usize),
    ])
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
static TODO_LENGTH: u32 = 4u32;

// If this struct ever gets pushed outside of this module, it may be worth creating an enum for
// the different tag types + other convenience methods.
/// Represents a TODO tag or any of its variants - FIXME, XXX, BUG, TODO.
#[derive(Debug, PartialEq, Eq)]
struct Tag<'a> {
    range: TextRange,
    content: &'a str,
}

pub fn check_todos(
    tokens: &[LexResult],
    autofix: flags::Autofix,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let mut iter = tokens.iter().flatten().peekable();
    while let Some((token, token_range)) = iter.next() {
        let Tok::Comment(comment) = token else {
            continue;
        };

        // Check that the comment is a TODO (properly formed or not).
        let Some(tag) = detect_tag(comment, token_range) else {
            continue;
        };

        check_for_tag_errors(token_range, &tag, &mut diagnostics, autofix, settings);
        check_for_static_errors(comment, token_range, &tag, &mut diagnostics);

        // TDO-003
        let todo_start = tag.range.start();
        if let Some((next_token, _next_range)) = iter.peek() {
            if let Tok::Comment(next_comment) = next_token {
                if ISSUE_LINK_REGEX_SET.is_match(next_comment) {
                    continue;
                }
            }

            diagnostics.push(Diagnostic::new(MissingLinkInTodo, tag.range));
        } else {
            // There's a TODO on the last line of the file, so there can't be a link after it.
            diagnostics.push(Diagnostic::new(MissingLinkInTodo, tag.range));
        }
    }

    diagnostics
}

/// Returns the tag pulled out of a given comment if it exists.
fn detect_tag<'a>(comment: &'a String, comment_range: &'a TextRange) -> Option<Tag<'a>> {
    let Some(regex_index) = TODO_REGEX_SET.matches(comment).into_iter().next() else {
        return None;
    };

    let tag_length = *PATTERN_TAG_LENGTH.get(&regex_index).unwrap();

    let mut tag_start_offset = 0usize;
    for (i, char) in comment.chars().enumerate() {
        // Regex ensures that the first letter in the comment is the first letter of the tag.
        if char.is_alphabetic() {
            tag_start_offset = i;
            break;
        }
    }

    Some(Tag {
        content: comment
            .get(tag_start_offset..tag_start_offset + tag_length)
            .unwrap(),
        range: TextRange::at(
            comment_range.start() + TextSize::try_from(tag_start_offset).ok().unwrap(),
            TextSize::try_from(tag_length).ok().unwrap(),
        ),
    })
}

/// Check that the tag is valid since we're not using capture groups anymore. This function
/// modifies `diagnostics` in-place.
fn check_for_tag_errors(
    comment_range: &TextRange,
    tag: &Tag,
    diagnostics: &mut Vec<Diagnostic>,
    autofix: flags::Autofix,
    settings: &Settings,
) {
    if tag.content != "TODO" {
        // TDO-001
        diagnostics.push(Diagnostic::new(
            InvalidTodoTag {
                tag: tag.content.to_string(),
            },
            tag.range,
        ));

        if tag.content.to_uppercase() == "TODO" {
            // TDO-006
            let mut invalid_capitalization = Diagnostic::new(
                InvalidCapitalizationInTodo {
                    tag: tag.content.to_string(),
                },
                tag.range,
            );

            if autofix.into() && settings.rules.should_fix(Rule::InvalidCapitalizationInTodo) {
                invalid_capitalization.set_fix(Fix::new(vec![Edit::replacement(
                    "TODO".to_string(),
                    tag.range.start(),
                    tag.range.end(),
                )]));
            }

            diagnostics.push(invalid_capitalization);
        }
    }
}

/// Checks for "static" errors in the comment - missing colon, missing author, etc. This function
/// modifies `diagnostics` in-place.
fn check_for_static_errors(
    comment: &str,
    comment_range: &TextRange,
    tag: &Tag,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Relative offset of the current character vs.the comment range, after we skip the tag.
    let mut relative_offset: usize = usize::try_from(tag.range.end() - comment_range.start())
        .ok()
        .unwrap();
    let mut comment_chars = comment.chars().skip(relative_offset).peekable();
    // Absolute offset of the comment's author block from the start of the file.
    let mut author_range: Option<TextRange> = None;
    // Absolute offset of the comment's colon from the start of the file.
    let mut colon_offset: Option<TextSize> = None;

    // An "author block" must be contained in parantheses, like "(ruff)". To check if it exists,
    // we can check the first non-whitespace character after the tag. If that first character is a
    // left parenthesis, we can say that we have an author's block.
    while let Some(char) = comment_chars.next() {
        relative_offset += 1;
        if char.is_whitespace() {
            continue;
        }

        // We can guarantee that there's no author if the colon directly follows the TODO tag.
        if char == ':' {
            colon_offset =
                Some(comment_range.start() + TextSize::try_from(relative_offset).ok().unwrap());
            break;
        }

        if char == '(' {
            author_range = Some(TextRange::at(
                comment_range.start() + TextSize::try_from(relative_offset).ok().unwrap(),
                TextSize::new(1),
            ));
        }

        break;
    }

    if let Some(range) = author_range {
        let mut author_block_length = 0usize;
        while let Some(char) = comment_chars.next() {
            relative_offset += 1;
            author_block_length += 1;

            if char == ')' {
                break;
            }
        }

        author_range = Some(range.add_end(TextSize::try_from(author_block_length).ok().unwrap()));
    } else {
        diagnostics.push(Diagnostic::new(
            MissingAuthorInTodo,
            TextRange::at(tag.range.end(), TextSize::new(1)),
        ));
    }

    // A valid colon must be the character after the author block. If the author block doesn't
    // exist, the colon must directly follow the tag.
    if let Some(char) = comment_chars.next() {
        relative_offset += 1;

        if char == ':' {
            colon_offset =
                Some(comment_range.start() + TextSize::try_from(relative_offset).ok().unwrap());
        }
    }

    if let Some(range) = colon_offset {
        match comment_chars.next() {
            Some(char) => {
                if char == ' ' {
                    return;
                }

                diagnostics.push(Diagnostic::new(
                    MissingSpaceAfterColonInTodo,
                    TextRange::at(range, TextSize::new(1)),
                ));
            }
            None => {}
        }
    } else {
        // Adjust where the colon should be based on the length of the author block, if it exists.
        let adjusted_colon_position = tag.range.end()
            + if author_range.is_some() {
                author_range.unwrap().len()
            } else {
                TextSize::new(0)
            };

        diagnostics.push(Diagnostic::new(
            MissingColonInTodo,
            TextRange::at(adjusted_colon_position, TextSize::new(1)),
        ))
    }

    match comment_chars.next() {
        Some(_) => {}
        None => diagnostics.push(Diagnostic::new(
            MissingTextInTodo,
            TextRange::at(comment_range.end(), TextSize::new(1)),
        )),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_tag() {
        let test_comment = "# TODO: todo tag";
        let mut chars = test_comment.chars();
        let expected = Tag {
            content: "TODO",
            range: TextRange::new(TextSize::new(2), TextSize::new(6)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                &test_comment.to_owned(),
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );

        let test_comment = "#TODO: todo tag";
        let mut chars = test_comment.chars();
        let expected = Tag {
            content: "TODO",
            range: TextRange::new(TextSize::new(1), TextSize::new(5)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                &test_comment.to_owned(),
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );

        // sanity checks :)
        let test_comment = "# todo: todo tag";
        let mut chars = test_comment.chars();
        let expected = Tag {
            content: "todo",
            range: TextRange::new(TextSize::new(2), TextSize::new(6)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                &test_comment.to_owned(),
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );
        let test_comment = "# fixme: fixme tag";
        let mut chars = test_comment.chars();
        let expected = Tag {
            content: "fixme",
            range: TextRange::new(TextSize::new(2), TextSize::new(7)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                &test_comment.to_owned(),
                &TextRange::new(TextSize::new(0), TextSize::new(17)),
            )
        );
    }
}
