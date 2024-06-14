use crate::red_knot::semantic_index::ast_ids::{
    LocalAnnotatedAssignmentId, LocalAssignmentId, LocalClassId, LocalFunctionId,
    LocalImportFromId, LocalImportId, LocalNamedExprId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Definition {
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(LocalClassId),
    FunctionDef(LocalFunctionId),
    Assignment(LocalAssignmentId),
    AnnotatedAssignment(LocalAnnotatedAssignmentId),
    NamedExpr(LocalNamedExprId),
    /// represents the implicit initial definition of every name as "unbound"
    Unbound,
    // TODO with statements, except handlers, function args...
}

impl From<LocalClassId> for Definition {
    fn from(value: LocalClassId) -> Self {
        Self::ClassDef(value)
    }
}

impl From<LocalFunctionId> for Definition {
    fn from(value: LocalFunctionId) -> Self {
        Self::FunctionDef(value)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ImportDefinition {
    pub(super) import_id: LocalImportId,

    /// Index into [`ruff_python_ast::StmtImport::names`].
    pub(super) alias: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ImportFromDefinition {
    pub(super) import_id: LocalImportFromId,

    /// Index into [`ruff_python_ast::StmtImport::names`].
    pub(super) name: u32,
}
