use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::docstrings::{clean_space, leading_space};
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for docstrings that are indented with tabs.
///
/// ## Why is this bad?
/// [PEP 8](https://peps.python.org/pep-0008/#tabs-or-spaces) recommends using
/// spaces over tabs for indentation.
///
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
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
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
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct UnderIndentation;

impl AlwaysAutofixableViolation for UnderIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is under-indented")
    }

    fn autofix_title(&self) -> String {
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
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct OverIndentation;

impl AlwaysAutofixableViolation for OverIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is over-indented")
    }

    fn autofix_title(&self) -> String {
        "Remove over-indentation".to_string()
    }
}

/// D206, D207, D208
pub(crate) fn indent(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    // Split the docstring into lines.
    let lines: Vec<_> = NewlineWithTrailingNewline::with_offset(&body, body.start()).collect();
    if lines.len() <= 1 {
        return;
    }

    let mut has_seen_tab = docstring.indentation.contains('\t');
    let mut is_over_indented = true;
    let mut over_indented_lines = vec![];

    for i in 0..lines.len() {
        // First lines and continuations doesn't need any indentation.
        if i == 0 || lines[i - 1].ends_with('\\') {
            continue;
        }

        let line = &lines[i];
        // Omit empty lines, except for the last line, which is non-empty by way of
        // containing the closing quotation marks.
        let is_blank = line.trim().is_empty();
        if i < lines.len() - 1 && is_blank {
            continue;
        }

        let line_indent = leading_space(line);

        // We only report tab indentation once, so only check if we haven't seen a tab
        // yet.
        has_seen_tab = has_seen_tab || line_indent.contains('\t');

        if checker.enabled(Rule::UnderIndentation) {
            // We report under-indentation on every line. This isn't great, but enables
            // autofix.
            if (i == lines.len() - 1 || !is_blank)
                && line_indent.len() < docstring.indentation.len()
            {
                let mut diagnostic =
                    Diagnostic::new(UnderIndentation, TextRange::empty(line.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                        clean_space(docstring.indentation),
                        TextRange::at(line.start(), line_indent.text_len()),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        // Like pydocstyle, we only report over-indentation if either: (1) every line
        // (except, optionally, the last line) is over-indented, or (2) the last line
        // (which contains the closing quotation marks) is
        // over-indented. We can't know if we've achieved that condition
        // until we've viewed all the lines, so for now, just track
        // the over-indentation status of every line.
        if i < lines.len() - 1 {
            if line_indent.len() > docstring.indentation.len() {
                over_indented_lines.push(TextRange::at(line.start(), line_indent.text_len()));
            } else {
                is_over_indented = false;
            }
        }
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
        if is_over_indented {
            for over_indented in over_indented_lines {
                // We report over-indentation on every line. This isn't great, but
                // enables autofix.
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(over_indented.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    let indent = clean_space(docstring.indentation);
                    let edit = if indent.is_empty() {
                        Edit::range_deletion(over_indented)
                    } else {
                        Edit::range_replacement(indent, over_indented)
                    };
                    diagnostic.set_fix(Fix::automatic(edit));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        // If the last line is over-indented...
        if let Some(last) = lines.last() {
            let line_indent = leading_space(last);
            if line_indent.len() > docstring.indentation.len() {
                let mut diagnostic =
                    Diagnostic::new(OverIndentation, TextRange::empty(last.start()));
                if checker.patch(diagnostic.kind.rule()) {
                    let indent = clean_space(docstring.indentation);
                    let range = TextRange::at(last.start(), line_indent.text_len());
                    let edit = if indent.is_empty() {
                        Edit::range_deletion(range)
                    } else {
                        Edit::range_replacement(indent, range)
                    };
                    diagnostic.set_fix(Fix::automatic(edit));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
