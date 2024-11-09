use ruff_notebook::CellOffsets;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;

/// Return `true` if the statement containing the current expression is the last
/// top-level expression in the cell. This assumes that the source is a Jupyter
/// Notebook.
pub(super) fn at_last_top_level_expression_in_cell(
    semantic: &SemanticModel,
    locator: &Locator,
    cell_offsets: Option<&CellOffsets>,
) -> bool {
    if !semantic.at_top_level() {
        return false;
    }
    let current_statement_end = semantic.current_statement().end();
    cell_offsets
        .and_then(|cell_offsets| cell_offsets.containing_range(current_statement_end))
        .is_some_and(|cell_range| {
            SimpleTokenizer::new(
                locator.contents(),
                TextRange::new(current_statement_end, cell_range.end()),
            )
            .all(|token| token.kind() == SimpleTokenKind::Semi || token.kind().is_trivia())
        })
}
