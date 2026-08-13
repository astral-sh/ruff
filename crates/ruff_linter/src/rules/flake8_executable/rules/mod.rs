use std::path::Path;

use ruff_python_trivia::CommentRanges;
pub(crate) use shebang_leading_whitespace::*;
pub(crate) use shebang_missing_executable_file::*;
pub(crate) use shebang_missing_python::*;
pub(crate) use shebang_not_executable::*;
pub(crate) use shebang_not_first_line::*;

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::comments::shebang::ShebangDirective;

mod shebang_leading_whitespace;
mod shebang_missing_executable_file;
mod shebang_missing_python;
mod shebang_not_executable;
mod shebang_not_first_line;

pub(crate) fn from_tokens(
    context: &LintContext,
    path: &Path,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    let mut has_any_shebang = false;
    for range in comment_ranges {
        let comment = locator.slice(range);
        if let Some(shebang) = ShebangDirective::try_extract(comment) {
            has_any_shebang = true;

            shebang_missing_python(range, &shebang, context);

            if context.is_rule_enabled(Rule::ShebangNotExecutable) {
                shebang_not_executable(path, range, context);
            }

            shebang_leading_whitespace(context, range, locator);

            shebang_not_first_line(range, locator, context);
        }
    }

    if !has_any_shebang {
        if context.is_rule_enabled(Rule::ShebangMissingExecutableFile) {
            shebang_missing_executable_file(path, context);
        }
    }
}
