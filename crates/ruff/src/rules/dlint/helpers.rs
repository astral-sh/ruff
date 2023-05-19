use rustpython_parser::ast;
use rustpython_parser::ast::Stmt;

#[derive(Debug, Copy, Clone)]
pub(crate) enum AnyStmtImport<'a> {
    Import(&'a ast::StmtImport),
    ImportFrom(&'a ast::StmtImportFrom),
}

impl<'a> AnyStmtImport<'a> {
    pub(crate) fn cast(stmt: &'a Stmt) -> Option<Self> {
        match stmt {
            Stmt::Import(import) => Some(Self::Import(import)),
            Stmt::ImportFrom(import_from) => Some(Self::ImportFrom(import_from)),
            _ => None,
        }
    }
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
