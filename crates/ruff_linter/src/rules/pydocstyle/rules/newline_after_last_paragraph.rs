use ruff_text_size::{TextLen, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::docstrings::clean_space;
use ruff_source_file::{NewlineWithTrailingNewline, UniversalNewlines};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for multi-line docstrings whose closing quotes are not on their
/// own line.
///
/// ## Why is this bad?
/// [PEP 257] recommends that the closing quotes of a multi-line docstring be
/// on their own line, for consistency and compatibility with documentation
/// tools that may need to parse the docstring.
///
/// ## Example
/// ```python
/// def sort_list(l: List[int]) -> List[int]:
///     """Return a sorted copy of the list.
///
///     Sort the list in ascending order and return a copy of the result using the
///     bubble sort algorithm."""
/// ```
///
/// Use instead:
/// ```python
/// def sort_list(l: List[int]) -> List[int]:
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
pub struct NewLineAfterLastParagraph;

impl AlwaysFixableViolation for NewLineAfterLastParagraph {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring closing quotes should be on a separate line")
    }

    fn fix_title(&self) -> String {
        "Move closing quotes to new line".to_string()
    }
}

/// D209
pub(crate) fn newline_after_last_paragraph(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body();

    if !docstring.triple_quoted() {
        return;
    }

    let mut line_count = 0;
    for line in NewlineWithTrailingNewline::from(body.as_str()) {
        if !line.trim().is_empty() {
            line_count += 1;
        }
        if line_count > 1 {
            if let Some(last_line) = contents
                .universal_newlines()
                .last()
                .map(|l| l.as_str().trim())
            {
                if last_line != "\"\"\"" && last_line != "'''" {
                    let mut diagnostic =
                        Diagnostic::new(NewLineAfterLastParagraph, docstring.range());
                    // Insert a newline just before the end-quote(s).
                    let num_trailing_quotes = "'''".text_len();
                    let num_trailing_spaces: TextSize = last_line
                        .chars()
                        .rev()
                        .skip(usize::from(num_trailing_quotes))
                        .take_while(|c| c.is_whitespace())
                        .map(TextLen::text_len)
                        .sum();
                    let content = format!(
                        "{}{}",
                        checker.stylist().line_ending().as_str(),
                        clean_space(docstring.indentation)
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                        content,
                        docstring.end() - num_trailing_quotes - num_trailing_spaces,
                        docstring.end() - num_trailing_quotes,
                    )));
                    checker.diagnostics.push(diagnostic);
                }
            }
            return;
        }
    }
}
