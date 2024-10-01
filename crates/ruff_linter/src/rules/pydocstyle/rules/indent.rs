use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::docstrings::{clean_space, leading_space};
use ruff_source_file::{Line, NewlineWithTrailingNewline};
use ruff_text_size::{Ranged, TextSize};
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::Rule;

/// ## What it does
/// Checks for docstrings that are indented with tabs.
///
/// ## Why is this bad?
/// [PEP 8] recommends using spaces over tabs for indentation.
///
/// ## Example
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
/// 	Sort the list in ascending order and return a copy of the result using the bubble
/// 	sort algorithm.
///     """
/// ```
///
/// Use instead:
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
///     Sort the list in ascending order and return a copy of the result using the bubble
///     sort algorithm.
///     """
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// The rule is also incompatible with the [formatter] when using
/// `format.indent-style="tab"`.
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 8]: https://peps.python.org/pep-0008/#tabs-or-spaces
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct IndentWithSpaces;

impl Violation for IndentWithSpaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring should be indented with spaces, not tabs")
    }
}

/// ## What it does
/// Checks for under-indented docstrings.
///
/// ## Why is this bad?
/// [PEP 257] recommends that docstrings be indented to the same level as their
/// opening quotes. Avoid under-indenting docstrings, for consistency.
///
/// ## Example
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
/// Sort the list in ascending order and return a copy of the result using the bubble sort
/// algorithm.
///     """
/// ```
///
/// Use instead:
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
///     Sort the list in ascending order and return a copy of the result using the bubble
///     sort algorithm.
///     """
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
/// [formatter]: https://docs.astral.sh/ruff/formatter/
#[violation]
pub struct UnderIndentation;

impl AlwaysFixableViolation for UnderIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is under-indented")
    }

    fn fix_title(&self) -> String {
        "Increase indentation".to_string()
    }
}

/// ## What it does
/// Checks for over-indented docstrings.
///
/// ## Why is this bad?
/// [PEP 257] recommends that docstrings be indented to the same level as their
/// opening quotes. Avoid over-indenting docstrings, for consistency.
///
/// ## Example
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
///         Sort the list in ascending order and return a copy of the result using the
///         bubble sort algorithm.
///     """
/// ```
///
/// Use instead:
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
///     Sort the list in ascending order and return a copy of the result using the bubble
///     sort algorithm.
///     """
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[violation]
pub struct OverIndentation;

impl AlwaysFixableViolation for OverIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is over-indented")
    }

    fn fix_title(&self) -> String {
        "Remove over-indentation".to_string()
    }
}

/// D206, D207, D208
pub(crate) fn indent(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    // Split the docstring into lines.
    let mut lines = NewlineWithTrailingNewline::with_offset(&body, body.start()).peekable();

    // The current line being processed
    let mut current: Option<Line> = lines.next();

    if lines.peek().is_none() {
        return;
    }

    let mut has_seen_tab = docstring.indentation.contains('\t');
    let docstring_indent_size = docstring.indentation.chars().count();

    // Lines, other than the last, that are over indented.
    let mut over_indented_lines = vec![];
    // The smallest over indent that all docstring lines have in common. None if any line isn't over indented.
    let mut smallest_over_indent_size = Some(usize::MAX);
    // The last processed line
    let mut last = None;

    while let Some(line) = current {
        // First lines and continuations don't need any indentation.
        if last.is_none()
            || last
                .as_deref()
                .is_some_and(|last: &str| last.ends_with('\\'))
        {
            last = Some(line);
            current = lines.next();
            continue;
        }

        let is_last = lines.peek().is_none();

        // Omit empty lines, except for the last line, which is non-empty by way of
        // containing the closing quotation marks.
        let is_blank = line.trim().is_empty();
        if !is_last && is_blank {
            last = Some(line);
            current = lines.next();
            continue;
        }

        let line_indent = leading_space(&line);
        let line_indent_size = line_indent.chars().count();

        // We only report tab indentation once, so only check if we haven't seen a tab
        // yet.
        has_seen_tab = has_seen_tab || line_indent.contains('\t');

        if checker.enabled(Rule::UnderIndentation) {
            // We report under-indentation on every line. This isn't great, but enables
            // fix.
            if (is_last || !is_blank) && line_indent_size < docstring_indent_size {
                let mut diagnostic =
                    Diagnostic::new(UnderIndentation, TextRange::empty(line.start()));
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    clean_space(docstring.indentation),
                    TextRange::at(line.start(), line_indent.text_len()),
                )));
                checker.diagnostics.push(diagnostic);
            }
        }

        // Only true when the last line is indentation only followed by the closing quotes.
        // False when it is not the last line or the last line contains content other than the closing quotes.
        // The last line only has special handling when it contains no other content.
        let is_last_closing_quotes_only = is_last && is_blank;

        // Like pydocstyle, we only report over-indentation if either: (1) every line
        // (except, optionally, the last line) is over-indented, or (2) the last line
        // (which contains the closing quotation marks) is
        // over-indented. We can't know if we've achieved that condition
        // until we've viewed all the lines, so for now, just track
        // the over-indentation status of every line.
        if !is_last_closing_quotes_only {
            smallest_over_indent_size =
                smallest_over_indent_size.and_then(|smallest_over_indent_size| {
                    let over_indent_size = line_indent_size.saturating_sub(docstring_indent_size);

                    // `docstring_indent_size < line_indent_size`
                    if over_indent_size > 0 {
                        over_indented_lines.push(line.clone());
                        // Track the _smallest_ offset we see, in terms of characters.
                        Some(smallest_over_indent_size.min(over_indent_size))
                    } else {
                        None
                    }
                });
        }

        last = Some(line);
        current = lines.next();
    }

    if checker.enabled(Rule::IndentWithSpaces) {
        if has_seen_tab {
            checker
                .diagnostics
                .push(Diagnostic::new(IndentWithSpaces, docstring.range()));
        }
    }

    if checker.enabled(Rule::OverIndentation) {
        // If every line (except the last) is over-indented...
        if let Some(smallest_over_indent_size) = smallest_over_indent_size {
            for line in over_indented_lines {
                let line_indent = leading_space(&line);
                let indent = clean_space(docstring.indentation);

                // We report over-indentation on every line. This isn't great, but
                // enables the fix capability.
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(line.start()));

                let edit = if indent.is_empty() {
                    // Delete the entire indent.
                    Edit::range_deletion(TextRange::at(line.start(), line_indent.text_len()))
                } else {
                    // Convert the character count to an offset within the source.
                    // Example, where `[]` is a 2 byte non-breaking space:
                    // ```
                    // def f():
                    //     """ Docstring header
                    // ^^^^ Real indentation is 4 chars
                    //       docstring body, over-indented
                    // ^^^^^^ Over-indentation is 6 - 4 = 2 chars due to this line
                    //    [] []  docstring body 2, further indented
                    // ^^^^^ We take these 4 chars/5 bytes to match the docstring ...
                    //      ^^^ ... and these 2 chars/3 bytes to remove the `over_indented_size` ...
                    //         ^^ ... but preserve this real indent
                    // ```
                    let offset = checker
                        .locator()
                        .after(line.start())
                        .chars()
                        .take(docstring_indent_size + smallest_over_indent_size)
                        .map(TextLen::text_len)
                        .sum::<TextSize>();
                    let range = TextRange::at(line.start(), offset);
                    Edit::range_replacement(indent, range)
                };
                diagnostic.set_fix(Fix::safe_edit(edit));
                checker.diagnostics.push(diagnostic);
            }
        }

        // If the last line is over-indented...
        if let Some(last) = last {
            let line_indent = leading_space(&last);
            let line_indent_size = line_indent.chars().count();
            let last_line_over_indent = line_indent_size.saturating_sub(docstring_indent_size);

            let is_indent_only = line_indent.len() == last.len();
            if last_line_over_indent > 0 && is_indent_only {
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(last.start()));
                let indent = clean_space(docstring.indentation);
                let range = TextRange::at(last.start(), line_indent.text_len());
                let edit = if indent.is_empty() {
                    Edit::range_deletion(range)
                } else {
                    Edit::range_replacement(indent, range)
                };
                diagnostic.set_fix(Fix::safe_edit(edit));
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
