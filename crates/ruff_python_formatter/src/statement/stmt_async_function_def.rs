use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_python_ast::function::AnyFunctionDefinition;
use rustpython_parser::ast::StmtAsyncFunctionDef;

#[derive(Default)]
pub struct FormatStmtAsyncFunctionDef;

impl FormatNodeRule<StmtAsyncFunctionDef> for FormatStmtAsyncFunctionDef {
    fn fmt_fields(&self, item: &StmtAsyncFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        AnyFunctionDefinition::from(item).format().fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtAsyncFunctionDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled by `AnyFunctionDef`
        Ok(())
    }
}
