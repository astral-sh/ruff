use once_cell::sync::Lazy;
use regex::RegexSet;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::directives::{TodoComment, TodoDirective, TodoDirectiveKind};

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
        format!("Missing author in TODO; try: `# TODO(<author_name>): ...` or `# TODO @<author_name>: ...`")
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
/// # https://github.com/astral-sh/ruff/issues/3870
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

impl AlwaysFixableViolation for InvalidTodoCapitalization {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidTodoCapitalization { tag } = self;
        format!("Invalid TODO capitalization: `{tag}` should be `TODO`")
    }

    fn fix_title(&self) -> String {
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

static ISSUE_LINK_REGEX_SET: Lazy<RegexSet> = Lazy::new(|| {
    RegexSet::new([
        r"^#\s*(http|https)://.*", // issue link
        r"^#\s*\d+$",              // issue code - like "003"
        r"^#\s*[A-Z]{1,6}\-?\d+$", // issue code - like "TD003"
    ])
    .unwrap()
});

pub(crate) fn todos(
    diagnostics: &mut Vec<Diagnostic>,
    todo_comments: &[TodoComment],
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    for todo_comment in todo_comments {
        let TodoComment {
            directive,
            content,
            range_index,
            ..
        } = todo_comment;
        let range = todo_comment.range;

        // flake8-todos doesn't support HACK directives.
        if matches!(directive.kind, TodoDirectiveKind::Hack) {
            continue;
        }

        directive_errors(diagnostics, directive);
        static_errors(diagnostics, content, range, directive);

        let mut has_issue_link = false;
        let mut curr_range = range;
        for next_range in comment_ranges.iter().skip(range_index + 1).copied() {
            // Ensure that next_comment_range is in the same multiline comment "block" as
            // comment_range.
            if !locator
                .slice(TextRange::new(curr_range.end(), next_range.start()))
                .chars()
                .all(char::is_whitespace)
            {
                break;
            }

            let next_comment = locator.slice(next_range);
            if TodoDirective::from_comment(next_comment, next_range).is_some() {
                break;
            }

            if ISSUE_LINK_REGEX_SET.is_match(next_comment) {
                has_issue_link = true;
            }

            // If the next_comment isn't a tag or an issue, it's worthles in the context of this
            // linter. We can increment here instead of waiting for the next iteration of the outer
            // loop.
            curr_range = next_range;
        }

        if !has_issue_link {
            // TD003
            diagnostics.push(Diagnostic::new(MissingTodoLink, directive.range));
        }
    }
}

/// Check that the directive itself is valid. This function modifies `diagnostics` in-place.
fn directive_errors(diagnostics: &mut Vec<Diagnostic>, directive: &TodoDirective) {
    if directive.content == "TODO" {
        return;
    }

    if directive.content.to_uppercase() == "TODO" {
        // TD006
        let mut diagnostic = Diagnostic::new(
            InvalidTodoCapitalization {
                tag: directive.content.to_string(),
            },
            directive.range,
        );

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "TODO".to_string(),
            directive.range,
        )));

        diagnostics.push(diagnostic);
    } else {
        // TD001
        diagnostics.push(Diagnostic::new(
            InvalidTodoTag {
                tag: directive.content.to_string(),
            },
            directive.range,
        ));
    }
}

/// Checks for "static" errors in the comment: missing colon, missing author, etc.
fn static_errors(
    diagnostics: &mut Vec<Diagnostic>,
    comment: &str,
    comment_range: TextRange,
    directive: &TodoDirective,
) {
    let post_directive = &comment[usize::from(directive.range.end() - comment_range.start())..];
    let trimmed = post_directive.trim_start();
    let content_offset = post_directive.text_len() - trimmed.text_len();

    let author_end = content_offset
        + if trimmed.starts_with('(') {
            if let Some(end_index) = trimmed.find(')') {
                TextSize::try_from(end_index + 1).unwrap()
            } else {
                trimmed.text_len()
            }
        } else if trimmed.starts_with('@') {
            if let Some(end_index) = trimmed.find(|c: char| c.is_whitespace() || c == ':') {
                TextSize::try_from(end_index).unwrap()
            } else {
                // TD002
                diagnostics.push(Diagnostic::new(MissingTodoAuthor, directive.range));

                TextSize::new(0)
            }
        } else {
            // TD002
            diagnostics.push(Diagnostic::new(MissingTodoAuthor, directive.range));

            TextSize::new(0)
        };

    let after_author = &post_directive[usize::from(author_end)..];
    if let Some(after_colon) = after_author.strip_prefix(':') {
        if after_colon.is_empty() {
            // TD005
            diagnostics.push(Diagnostic::new(MissingTodoDescription, directive.range));
        } else if !after_colon.starts_with(char::is_whitespace) {
            // TD007
            diagnostics.push(Diagnostic::new(MissingSpaceAfterTodoColon, directive.range));
        }
    } else {
        // TD004
        diagnostics.push(Diagnostic::new(MissingTodoColon, directive.range));

        if after_author.is_empty() {
            // TD005
            diagnostics.push(Diagnostic::new(MissingTodoDescription, directive.range));
        }
    }
}
