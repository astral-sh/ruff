use rustpython_parser::ast;

#[derive(Debug, Copy, Clone)]
pub(crate) enum AnyStmtImport<'a> {
    Import(&'a ast::StmtImport),
    ImportFrom(&'a ast::StmtImportFrom),
}

impl<'a> From<&'a ast::StmtImport> for AnyStmtImport<'a> {
    fn from(value: &'a ast::StmtImport) -> Self {
        Self::Import(value)
    }
}

impl<'a> From<&'a ast::StmtImportFrom> for AnyStmtImport<'a> {
    fn from(value: &'a ast::StmtImportFrom) -> Self {
        Self::ImportFrom(value)
    }
}
