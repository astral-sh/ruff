use ruff_python_trivia::{
    BackwardsTokenizer, PythonWhitespace, SimpleToken, SimpleTokenKind, SimpleTokenizer,
};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

pub(super) fn comment_strings_in_range<'a>(
    checker: &'a Checker,
    range: TextRange,
    ranges_to_ignore: &[TextRange],
) -> Vec<&'a str> {
    checker
        .comment_ranges()
        .comments_in_range(range)
        .iter()
        // Ignore comments inside of the append or iterator, since these are preserved
        .filter(|comment| {
            !ranges_to_ignore
                .iter()
                .any(|to_ignore| to_ignore.contains_range(**comment))
        })
        .map(|range| checker.locator().slice(range).trim_whitespace_start())
        .collect()
}

fn semicolon_before_and_after(
    checker: &Checker,
    statement: TextRange,
) -> (Option<SimpleToken>, Option<SimpleToken>) {
    // determine whether there's a semicolon either before or after the binding statement.
    // Since it's a binding statement, we can just check whether there's a semicolon immediately
    // after the whitespace in front of or behind it
    let mut after_tokenizer =
        SimpleTokenizer::starts_at(statement.end(), checker.locator().contents()).skip_trivia();

    let after_semicolon = if after_tokenizer
        .next()
        .is_some_and(|token| token.kind() == SimpleTokenKind::Semi)
    {
        after_tokenizer.next()
    } else {
        None
    };

    let semicolon_before = BackwardsTokenizer::up_to(
        statement.start(),
        checker.locator().contents(),
        checker.comment_ranges(),
    )
    .skip_trivia()
    .next()
    .filter(|token| token.kind() == SimpleTokenKind::Semi);

    (semicolon_before, after_semicolon)
}

/// Finds the range necessary to delete a statement (including any semicolons around it).
/// Returns the range and whether there were multiple statements on the line
pub(super) fn statement_deletion_range(
    checker: &Checker,
    statement_range: TextRange,
) -> (TextRange, bool) {
    let locator = checker.locator();
    // If the binding has multiple statements on its line, the fix would be substantially more complicated
    let (semicolon_before, after_semicolon) = semicolon_before_and_after(checker, statement_range);

    // If there are multiple binding statements in one line, we don't want to accidentally delete them
    // Instead, we just delete the binding statement and leave any comments where they are

    match (semicolon_before, after_semicolon) {
        // ```python
        // a = []
        // ```
        (None, None) => (locator.full_lines_range(statement_range), false),

        // ```python
        // a = 1; b = []
        //      ^^^^^^^^
        // a = 1; b = []; c = 3
        //      ^^^^^^^^
        // ```
        (Some(semicolon_before), Some(_) | None) => (
            TextRange::new(semicolon_before.start(), statement_range.end()),
            true,
        ),

        // ```python
        // a = []; b = 3
        // ^^^^^^^
        // ```
        (None, Some(after_semicolon)) => (
            TextRange::new(statement_range.start(), after_semicolon.start()),
            true,
        ),
    }
}
