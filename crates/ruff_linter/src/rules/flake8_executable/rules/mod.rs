use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_python_trivia::CommentRanges;
pub(crate) use shebang_leading_whitespace::*;
pub(crate) use shebang_missing_executable_file::*;
pub(crate) use shebang_missing_python::*;
pub(crate) use shebang_not_executable::*;
pub(crate) use shebang_not_first_line::*;

use crate::codes::Rule;
use crate::comments::shebang::ShebangDirective;
use crate::settings::LinterSettings;
use crate::Locator;

mod shebang_leading_whitespace;
mod shebang_missing_executable_file;
mod shebang_missing_python;
mod shebang_not_executable;
mod shebang_not_first_line;

pub(crate) fn from_tokens(
    diagnostics: &mut Vec<Diagnostic>,
    path: &Path,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
) {
    let mut has_any_shebang = false;
    for range in comment_ranges {
        let comment = locator.slice(range);
        if let Some(shebang) = ShebangDirective::try_extract(comment) {
            has_any_shebang = true;

            if let Some(diagnostic) = shebang_missing_python(range, &shebang) {
                diagnostics.push(diagnostic);
            }

            if settings.rules.enabled(Rule::ShebangNotExecutable) {
                if let Some(diagnostic) = shebang_not_executable(path, range) {
                    diagnostics.push(diagnostic);
                }
            }

            if let Some(diagnostic) = shebang_leading_whitespace(range, locator) {
                diagnostics.push(diagnostic);
            }

            if let Some(diagnostic) = shebang_not_first_line(range, locator) {
                diagnostics.push(diagnostic);
            }
        }
    }

    if !has_any_shebang {
        if settings.rules.enabled(Rule::ShebangMissingExecutableFile) {
            if let Some(diagnostic) = shebang_missing_executable_file(path) {
                diagnostics.push(diagnostic);
            }
        }
    }
}
