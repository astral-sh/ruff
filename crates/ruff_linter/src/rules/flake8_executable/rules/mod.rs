use std::path::Path;

use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;

use ruff_diagnostics::Diagnostic;
use ruff_source_file::Locator;
pub(crate) use shebang_leading_whitespace::*;
pub(crate) use shebang_missing_executable_file::*;
pub(crate) use shebang_missing_python::*;
pub(crate) use shebang_not_executable::*;
pub(crate) use shebang_not_first_line::*;

use crate::comments::shebang::ShebangDirective;
use crate::settings::Settings;

mod shebang_leading_whitespace;
mod shebang_missing_executable_file;
mod shebang_missing_python;
mod shebang_not_executable;
mod shebang_not_first_line;

pub(crate) fn from_tokens(
    tokens: &[LexResult],
    path: &Path,
    locator: &Locator,
    settings: &Settings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut has_any_shebang = false;
    for (tok, range) in tokens.iter().flatten() {
        if let Tok::Comment(comment) = tok {
            if let Some(shebang) = ShebangDirective::try_extract(comment) {
                has_any_shebang = true;

                if let Some(diagnostic) = shebang_missing_python(*range, &shebang) {
                    diagnostics.push(diagnostic);
                }

                if let Some(diagnostic) = shebang_not_executable(path, *range) {
                    diagnostics.push(diagnostic);
                }

                if let Some(diagnostic) = shebang_leading_whitespace(*range, locator, settings) {
                    diagnostics.push(diagnostic);
                }

                if let Some(diagnostic) = shebang_not_first_line(*range, locator) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }

    if !has_any_shebang {
        if let Some(diagnostic) = shebang_missing_executable_file(path) {
            diagnostics.push(diagnostic);
        }
    }
}
