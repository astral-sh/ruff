use std::path::Path;

use ruff_python_trivia::{CommentRanges, is_python_whitespace};
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
            // Decide once whether this `#!` comment is a shebang at all. A shebang's `#!`
            // prefix only has meaning when it is the first thing on its line, or when nothing
            // but whitespace precedes it (an indented shebang is what EXE004 flags). A `#!`
            // that is indented and follows real code is an ordinary comment, so no shebang
            // rule should treat it as a directive.
            let prefix = locator.up_to(range.start());
            let at_line_start = prefix.is_empty() || prefix.ends_with(['\n', '\r']);
            let only_whitespace_before = prefix
                .chars()
                .all(|c| is_python_whitespace(c) || matches!(c, '\r' | '\n'));
            if !at_line_start && !only_whitespace_before {
                continue;
            }

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
