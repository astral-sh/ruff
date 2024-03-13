use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtImport;

use crate::comments::SourceComment;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatStmtImport;

impl FormatNodeRule<StmtImport> for FormatStmtImport {
    fn fmt_fields(&self, item: &StmtImport, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtImport { names, range: _ } = item;
        let names = format_with(|f| {
            f.join_with(&format_args![token(","), space()])
                .entries(names.iter().formatted())
                .finish()
        });
        write!(f, [token("import"), space(), names])
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
