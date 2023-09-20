use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

/// ## What it does
/// Checks for indentation that uses tabs.
///
/// ## Why is this bad?
/// According to [PEP 8], spaces are preferred over tabs (unless used to remain
/// consistent with code that is already indented with tabs).
///
/// ## Example
/// ```python
/// if True:
/// 	a = 1
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     a = 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#tabs-or-spaces
#[violation]
pub struct TabIndentation;

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

/// W191
pub(crate) fn tab_indentation(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &[LexResult],
    locator: &Locator,
    indexer: &Indexer,
) {
    // Always check the first line for tab indentation as there's no newline
    // token before it.
    tab_indentation_at_line_start(diagnostics, locator, TextSize::default());

    for (tok, range) in tokens.iter().flatten() {
        if matches!(tok, Tok::Newline | Tok::NonLogicalNewline) {
            tab_indentation_at_line_start(diagnostics, locator, range.end());
        }
    }

    // The lexer doesn't emit `Newline` / `NonLogicalNewline` for a line
    // continuation character (`\`), so we need to manually check for tab
    // indentation for lines that follow a line continuation character.
    for continuation_line in indexer.continuation_line_starts() {
        tab_indentation_at_line_start(
            diagnostics,
            locator,
            locator.full_line_end(*continuation_line),
        );
    }
}

/// Checks for indentation that uses tabs for a line starting at
/// the given [`TextSize`].
fn tab_indentation_at_line_start(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    line_start: TextSize,
) {
    let indent = leading_indentation(locator.after(line_start));
    if indent.find('\t').is_some() {
        diagnostics.push(Diagnostic::new(
            TabIndentation,
            TextRange::at(line_start, indent.text_len()),
        ));
    }
}
