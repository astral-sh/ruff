use crate::semantic_index::ast_ids::{
    ScopeAnnotatedAssignmentId, ScopeAssignmentId, ScopeClassId, ScopeFunctionId,
    ScopeImportFromId, ScopeImportId, ScopeNamedExprId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Definition {
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(ScopeClassId),
    FunctionDef(ScopeFunctionId),
    Assignment(ScopeAssignmentId),
    AnnotatedAssignment(ScopeAnnotatedAssignmentId),
    NamedExpr(ScopeNamedExprId),
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

impl From<ScopeClassId> for Definition {
    fn from(value: ScopeClassId) -> Self {
        Self::ClassDef(value)
    }
}

impl From<ScopeFunctionId> for Definition {
    fn from(value: ScopeFunctionId) -> Self {
        Self::FunctionDef(value)
    }
}

impl From<ScopeAssignmentId> for Definition {
    fn from(value: ScopeAssignmentId) -> Self {
        Self::Assignment(value)
    }
}

impl From<ScopeAnnotatedAssignmentId> for Definition {
    fn from(value: ScopeAnnotatedAssignmentId) -> Self {
        Self::AnnotatedAssignment(value)
    }
}

impl From<ScopeNamedExprId> for Definition {
    fn from(value: ScopeNamedExprId) -> Self {
        Self::NamedExpr(value)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ImportDefinition {
    pub(crate) import_id: ScopeImportId,

    /// Index into [`ruff_python_ast::StmtImport::names`].
    pub(crate) alias: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ImportFromDefinition {
    pub(crate) import_id: ScopeImportFromId,

    /// Index into [`ruff_python_ast::StmtImportFrom::names`].
    pub(crate) name: u32,
}
