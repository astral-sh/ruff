use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtImport;

use crate::comments::format::empty_lines_before_trailing_comments;
use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtImport;

impl FormatNodeRule<StmtImport> for FormatStmtImport {
    fn fmt_fields(&self, item: &StmtImport, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtImport { names, range: _ } = item;
        let names = format_with(|f| {
            f.join_with(&format_args![text(","), space()])
                .entries(names.iter().formatted())
                .finish()
        });
        write!(f, [text("import"), space(), names])?;

        let comments = f.context().comments().clone();

        // If the import contains trailing comments, insert a newline before them.
        // For example, given:
        // ```python
        // import module
        // # comment
        // ```
        //
        // At the top-level, reformat as:
        // ```python
        // import module
        //
        // # comment
        // ```
        empty_lines_before_trailing_comments(comments.trailing(item), 1).fmt(f)?;

        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
