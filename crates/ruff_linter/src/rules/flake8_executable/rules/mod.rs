use std::path::Path;

pub(crate) use executable_file_without_shebang::*;
pub(crate) use non_executable_file_with_shebang::*;
use ruff_python_trivia::CommentRanges;
pub(crate) use shebang_leading_whitespace::*;
pub(crate) use shebang_missing_python::*;
pub(crate) use shebang_not_first_line::*;

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::comments::shebang::ShebangDirective;

mod executable_file_without_shebang;
mod non_executable_file_with_shebang;
mod shebang_leading_whitespace;
mod shebang_missing_python;
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

            if context.is_rule_enabled(Rule::NonExecutableFileWithShebang) {
                non_executable_file_with_shebang(path, range, context);
            }

            shebang_leading_whitespace(context, range, locator);

            shebang_not_first_line(range, locator, context);
        }
    }

    if !has_any_shebang {
        if context.is_rule_enabled(Rule::ExecutableFileWithoutShebang) {
            executable_file_without_shebang(path, context);
        }
    }
}
