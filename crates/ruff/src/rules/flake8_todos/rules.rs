use once_cell::sync::Lazy;
use regex::RegexSet;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::{registry::Rule, settings::Settings};

/// ## What it does
/// Checks that a TODO comment is labelled with "TODO".
///
/// ## Why is this bad?
/// Ambiguous tags reduce code visibility and can lead to dangling TODOs.
/// For example, if a comment is tagged with "FIXME" rather than "TODO", it may
/// be overlooked by future readers.
///
/// Note that this rule will only flag "FIXME" and "XXX" tags as incorrect.
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
        format!("Invalid TODO tag: `{tag}`")
    }
}

/// ## What it does
/// Checks that a TODO comment includes an author.
///
/// ## Why is this bad?
/// Including an author on a TODO provides future readers with context around
/// the issue. While the TODO author is not always considered responsible for
/// fixing the issue, they are typically the individual with the most context.
///
/// ## Example
/// ```python
/// # TODO: should assign an author here
/// ```
///
/// Use instead
/// ```python
/// # TODO(charlie): now an author is assigned
/// ```
#[violation]
pub struct MissingTodoAuthor;

impl Violation for MissingTodoAuthor {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing author in TODO; try: `# TODO(<author_name>): ...`")
    }
}

/// ## What it does
/// Checks that a TODO comment is associated with a link to a relevant issue
/// or ticket.
///
/// ## Why is this bad?
/// Including an issue link near a TODO makes it easier for resolvers
/// to get context around the issue.
///
/// ## Example
/// ```python
/// # TODO: this link has no issue
/// ```
///
/// Use one of these instead:
/// ```python
/// # TODO(charlie): this comment has an issue link
/// # https://github.com/charliermarsh/ruff/issues/3870
///
/// # TODO(charlie): this comment has a 3-digit issue code
/// # 003
///
/// # TODO(charlie): this comment has an issue code of (up to) 6 characters, then digits
/// # SIXCHR-003
/// ```
#[violation]
pub struct MissingTodoLink;

impl Violation for MissingTodoLink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing issue link on the line following this TODO")
    }
}

/// ## What it does
/// Checks that a "TODO" tag is followed by a colon.
///
/// ## Why is this bad?
/// "TODO" tags are typically followed by a parenthesized author name, a colon,
/// a space, and a description of the issue, in that order.
///
/// Deviating from this pattern can lead to inconsistent and non-idiomatic
/// comments.
///
/// ## Example
/// ```python
/// # TODO(charlie) fix this colon
/// ```
///
/// Used instead:
/// ```python
/// # TODO(charlie): colon fixed
/// ```
#[violation]
pub struct MissingTodoColon;

impl Violation for MissingTodoColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing colon in TODO")
    }
}

/// ## What it does
/// Checks that a "TODO" tag contains a description of the issue following the
/// tag itself.
///
/// ## Why is this bad?
/// TODO comments should include a description of the issue to provide context
/// for future readers.
///
/// ## Example
/// ```python
/// # TODO(charlie)
/// ```
///
/// Use instead:
/// ```python
/// # TODO(charlie): fix some issue
/// ```
#[violation]
pub struct MissingTodoDescription;

impl Violation for MissingTodoDescription {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing issue description after `TODO`")
    }
}

/// ## What it does
/// Checks that a "TODO" tag is properly capitalized (i.e., that the tag is
/// uppercase).
///
/// ## Why is this bad?
/// Capitalizing the "TODO" in a TODO comment is a convention that makes it
/// easier for future readers to identify TODOs.
///
/// ## Example
/// ```python
/// # todo(charlie): capitalize this
/// ```
///
/// Use instead:
/// ```python
/// # TODO(charlie): this is capitalized
/// ```
#[violation]
pub struct InvalidTodoCapitalization {
    tag: String,
}

impl AlwaysAutofixableViolation for InvalidTodoCapitalization {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidTodoCapitalization { tag } = self;
        format!("Invalid TODO capitalization: `{tag}` should be `TODO`")
    }

    fn autofix_title(&self) -> String {
        let InvalidTodoCapitalization { tag } = self;
        format!("Replace `{tag}` with `TODO`")
    }
}

/// ## What it does
/// Checks that the colon after a "TODO" tag is followed by a space.
///
/// ## Why is this bad?
/// "TODO" tags are typically followed by a parenthesized author name, a colon,
/// a space, and a description of the issue, in that order.
///
/// Deviating from this pattern can lead to inconsistent and non-idiomatic
/// comments.
///
/// ## Example
/// ```python
/// # TODO(charlie):fix this
/// ```
///
/// Use instead:
/// ```python
/// # TODO(charlie): fix this
/// ```
#[violation]
pub struct MissingSpaceAfterTodoColon;

impl Violation for MissingSpaceAfterTodoColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing space after colon in TODO")
    }
}

static TODO_REGEX_SET: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        r#"^#\s*(?i)(TODO).*$"#,
        r#"^#\s*(?i)(FIXME).*$"#,
        r#"^#\s*(?i)(XXX).*$"#,
    ])
    .unwrap()
});

// Maps the index of a particular Regex (specified by its index in the above TODO_REGEX_SET slice)
// to the length of the tag that we're trying to capture.
static PATTERN_TAG_LENGTH: &[usize; 3] = &["TODO".len(), "FIXME".len(), "XXX".len()];

static ISSUE_LINK_REGEX_SET: Lazy<RegexSet> = Lazy::new(|| {
    let patterns: [&str; 3] = [
        r#"^#\s*(http|https)://.*"#, // issue link
        r#"^#\s*\d+$"#,              // issue code - like "003"
        r#"^#\s*[A-Z]{1,6}\-?\d+$"#, // issue code - like "TD003" or "TD-003"
    ];
    RegexSet::new(patterns).unwrap()
});

// If this struct ever gets pushed outside of this module, it may be worth creating an enum for
// the different tag types + other convenience methods.
/// Represents a TODO tag or any of its variants - FIXME, XXX, BUG, TODO.
#[derive(Debug, PartialEq, Eq)]
struct Tag<'a> {
    range: TextRange,
    content: &'a str,
}

pub(crate) fn todos(tokens: &[LexResult], settings: &Settings) -> Vec<Diagnostic> {
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

        check_for_tag_errors(&tag, &mut diagnostics, settings);
        check_for_static_errors(&mut diagnostics, comment, *token_range, &tag);

        // TD003
        if let Some((next_token, _next_range)) = iter.peek() {
            if let Tok::Comment(next_comment) = next_token {
                if ISSUE_LINK_REGEX_SET.is_match(next_comment) {
                    continue;
                }
            }

            diagnostics.push(Diagnostic::new(MissingTodoLink, tag.range));
        } else {
            // There's a TODO on the last line of the file, so there can't be a link after it.
            diagnostics.push(Diagnostic::new(MissingTodoLink, tag.range));
        }
    }

    diagnostics
}

/// Returns the tag pulled out of a given comment, if it exists.
fn detect_tag<'a>(comment: &'a str, comment_range: &'a TextRange) -> Option<Tag<'a>> {
    let Some(regex_index) = TODO_REGEX_SET.matches(comment).into_iter().next() else {
        return None;
    };

    let tag_length = PATTERN_TAG_LENGTH[regex_index];

    let mut tag_start_offset = 0usize;
    for (i, char) in comment.chars().enumerate() {
        // Regex ensures that the first letter in the comment is the first letter of the tag.
        if char.is_alphabetic() {
            tag_start_offset = i;
            break;
        }
    }

    Some(Tag {
        content: &comment[tag_start_offset..tag_start_offset + tag_length],
        range: TextRange::at(
            comment_range.start() + TextSize::try_from(tag_start_offset).ok().unwrap(),
            TextSize::try_from(tag_length).ok().unwrap(),
        ),
    })
}

/// Check that the tag is valid. This function modifies `diagnostics` in-place.
fn check_for_tag_errors(tag: &Tag, diagnostics: &mut Vec<Diagnostic>, settings: &Settings) {
    if tag.content == "TODO" {
        return;
    }

    if tag.content.to_uppercase() == "TODO" {
        // TD006
        let mut invalid_capitalization = Diagnostic::new(
            InvalidTodoCapitalization {
                tag: tag.content.to_string(),
            },
            tag.range,
        );

        if settings.rules.should_fix(Rule::InvalidTodoCapitalization) {
            invalid_capitalization.set_fix(Fix::automatic(Edit::range_replacement(
                "TODO".to_string(),
                tag.range,
            )));
        }

        diagnostics.push(invalid_capitalization);
    } else {
        // TD001
        diagnostics.push(Diagnostic::new(
            InvalidTodoTag {
                tag: tag.content.to_string(),
            },
            tag.range,
        ));
    }
}

/// Checks for "static" errors in the comment - missing colon, missing author, etc. This function
/// modifies `diagnostics` in-place.
fn check_for_static_errors(
    diagnostics: &mut Vec<Diagnostic>,
    comment: &str,
    comment_range: TextRange,
    tag: &Tag,
) {
    let post_tag = &comment[usize::from(tag.range.end() - comment_range.start())..];
    let trimmed = post_tag.trim_start();
    let content_offset = post_tag.text_len() - trimmed.text_len();

    let author_end = content_offset
        + if trimmed.starts_with('(') {
            if let Some(end_index) = trimmed.find(')') {
                TextSize::try_from(end_index + 1).unwrap()
            } else {
                trimmed.text_len()
            }
        } else {
            diagnostics.push(Diagnostic::new(MissingTodoAuthor, tag.range));

            TextSize::new(0)
        };

    let post_author = &post_tag[usize::from(author_end)..];

    let post_colon = if let Some((_colon, after_colon)) = post_author.split_once(':') {
        if let Some(stripped) = after_colon.strip_prefix(' ') {
            stripped
        } else {
            diagnostics.push(Diagnostic::new(MissingSpaceAfterTodoColon, tag.range));
            after_colon
        }
    } else {
        diagnostics.push(Diagnostic::new(MissingTodoColon, tag.range));
        ""
    };

    if post_colon.is_empty() {
        diagnostics.push(Diagnostic::new(MissingTodoDescription, tag.range));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_tag() {
        let test_comment = "# TODO: todo tag";
        let expected = Tag {
            content: "TODO",
            range: TextRange::new(TextSize::new(2), TextSize::new(6)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                test_comment,
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );

        let test_comment = "#TODO: todo tag";
        let expected = Tag {
            content: "TODO",
            range: TextRange::new(TextSize::new(1), TextSize::new(5)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                test_comment,
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );

        let test_comment = "# todo: todo tag";
        let expected = Tag {
            content: "todo",
            range: TextRange::new(TextSize::new(2), TextSize::new(6)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                test_comment,
                &TextRange::new(TextSize::new(0), TextSize::new(15)),
            )
        );
        let test_comment = "# fixme: fixme tag";
        let expected = Tag {
            content: "fixme",
            range: TextRange::new(TextSize::new(2), TextSize::new(7)),
        };
        assert_eq!(
            Some(expected),
            detect_tag(
                test_comment,
                &TextRange::new(TextSize::new(0), TextSize::new(17)),
            )
        );
    }
}
