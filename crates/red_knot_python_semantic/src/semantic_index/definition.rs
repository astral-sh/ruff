use crate::semantic_index::ast_ids::{
    ScopedClassId, ScopedExpressionId, ScopedFunctionId, ScopedStatementId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Definition {
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(ScopedClassId),
    FunctionDef(ScopedFunctionId),
    Assignment(ScopedStatementId),
    AnnotatedAssignment(ScopedStatementId),
    NamedExpr(ScopedExpressionId),
    /// represents the implicit initial definition of every name as "unbound"
    Unbound,
    // TODO with statements, except handlers, function args...
}

impl From<ImportDefinition> for Definition {
    fn from(value: ImportDefinition) -> Self {
        Self::Import(value)
    }
}

impl From<ImportFromDefinition> for Definition {
    fn from(value: ImportFromDefinition) -> Self {
        Self::ImportFrom(value)
    }
}

impl From<ScopedClassId> for Definition {
    fn from(value: ScopedClassId) -> Self {
        Self::ClassDef(value)
    }
}

impl From<ScopedFunctionId> for Definition {
    fn from(value: ScopedFunctionId) -> Self {
        Self::FunctionDef(value)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ImportDefinition {
    pub(crate) import_id: ScopedStatementId,

    /// Index into [`ruff_python_ast::StmtImport::names`].
    pub(crate) alias: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ImportFromDefinition {
    pub(crate) import_id: ScopedStatementId,

    /// Index into [`ruff_python_ast::StmtImportFrom::names`].
    pub(crate) name: u32,
}
