use std::path::Path;

use ruff_python_trivia::CommentRanges;
pub(crate) use shebang_leading_whitespace::*;
pub(crate) use shebang_missing_executable_file::*;
pub(crate) use shebang_missing_python::*;
pub(crate) use shebang_not_executable::*;
pub(crate) use shebang_not_first_line::*;

use crate::Locator;
use crate::checkers::ast::DiagnosticsCollector;
use crate::codes::Rule;
use crate::comments::shebang::ShebangDirective;
use crate::settings::LinterSettings;

mod shebang_leading_whitespace;
mod shebang_missing_executable_file;
mod shebang_missing_python;
mod shebang_not_executable;
mod shebang_not_first_line;

pub(crate) fn from_tokens(
    collector: &DiagnosticsCollector,
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

            shebang_missing_python(range, &shebang, collector);

            if settings.rules.enabled(Rule::ShebangNotExecutable) {
                shebang_not_executable(path, range, collector);
            }

            shebang_leading_whitespace(collector, range, locator);

            shebang_not_first_line(range, locator, collector);
        }
    }

    if !has_any_shebang {
        if settings.rules.enabled(Rule::ShebangMissingExecutableFile) {
            shebang_missing_executable_file(path, collector);
        }
    }
}
