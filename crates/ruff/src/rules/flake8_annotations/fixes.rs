use anyhow::{bail, Result};
use rustpython_parser::ast::{Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::Locator;

/// ANN204
pub(crate) fn add_return_annotation(
    locator: &Locator,
    stmt: &Stmt,
    annotation: &str,
) -> Result<Edit> {
    let contents = &locator.contents()[stmt.range()];

    // Find the colon (following the `def` keyword).
    let mut seen_lpar = false;
    let mut seen_rpar = false;
    let mut count = 0u32;
    for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, stmt.start()).flatten() {
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
