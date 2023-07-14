use crate::{FormatNodeRule, FormattedIterExt, PyFormatter};
use ruff_formatter::prelude::{format_args, format_with, space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtImport;

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
        write!(f, [text("import"), space(), names])
    }
}
