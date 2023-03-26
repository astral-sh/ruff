use anyhow::{bail, Result};
use rustpython_parser::ast::Stmt;
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

/// ANN204
pub fn add_return_annotation(locator: &Locator, stmt: &Stmt, annotation: &str) -> Result<Edit> {
    let range = Range::from(stmt);
    let contents = locator.slice(range);

    // Find the colon (following the `def` keyword).
    let mut seen_lpar = false;
    let mut seen_rpar = false;
    let mut count: usize = 0;
    for (start, tok, ..) in lexer::lex_located(contents, Mode::Module, range.location).flatten() {
        if seen_lpar && seen_rpar {
            if matches!(tok, Tok::Colon) {
                return Ok(Edit::insertion(format!(" -> {annotation}"), start));
            }
        }

        if matches!(tok, Tok::Lpar) {
            if count == 0 {
                seen_lpar = true;
            }
            count += 1;
        }
        if matches!(tok, Tok::Rpar) {
            count -= 1;
            if count == 0 {
                seen_rpar = true;
            }
        }
    }

    bail!("Unable to locate colon in function definition");
}
