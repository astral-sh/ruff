use anyhow::{bail, Result};
use ruff_python_ast::{PySourceType, Ranged};
use ruff_python_parser::{lexer, AsMode, Tok};

use ruff_diagnostics::Edit;
use ruff_source_file::Locator;

/// ANN204
pub(crate) fn add_return_annotation<T: Ranged>(
    statement: &T,
    annotation: &str,
    source_type: PySourceType,
    locator: &Locator,
) -> Result<Edit> {
    let contents = &locator.contents()[statement.range()];

    // Find the colon (following the `def` keyword).
    let mut seen_lpar = false;
    let mut seen_rpar = false;
    let mut count = 0u32;
    for (tok, range) in
        lexer::lex_starts_at(contents, source_type.as_mode(), statement.start()).flatten()
    {
        if seen_lpar && seen_rpar {
            if matches!(tok, Tok::Colon) {
                return Ok(Edit::insertion(format!(" -> {annotation}"), range.start()));
            }
        }

        if matches!(tok, Tok::Lpar) {
            if count == 0 {
                seen_lpar = true;
            }
            count = count.saturating_add(1);
        }
        if matches!(tok, Tok::Rpar) {
            count = count.saturating_sub(1);
            if count == 0 {
                seen_rpar = true;
            }
        }
    }

    bail!("Unable to locate colon in function definition");
}
