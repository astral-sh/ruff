use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtImport;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtImport;

impl FormatNodeRule<ruff_python_ast::kind::StmtImport> for FormatStmtImport {
    fn fmt_fields(&self, item: StmtImport<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let names = item.names();
        let is_lazy = item.is_lazy();
        let names = format_with(|f| {
            f.join_with(&format_args![token(","), space()])
                .entries(names.iter().formatted())
                .finish()
        });
        if is_lazy {
            write!(f, [token("lazy"), space()])?;
        }
        write!(f, [token("import"), space(), names])
    }
}
