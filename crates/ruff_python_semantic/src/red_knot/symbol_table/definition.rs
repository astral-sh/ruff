use crate::red_knot::ast_ids::{
    LocalAnnotatedAssignmentId, LocalAssignmentId, LocalClassId, LocalFunctionId,
    LocalImportFromId, LocalImportId,
};

// TODO should the AST ids be local to a scope or can they be global?
// Local:
// * Same as for other IDs, might be easier to understand
// * Can be built as part of the semantic indexing
// * IDs don't change when the body of a scope isn't changing.
//
// global:
// * Lookup is much easier
// * We can store `ExpressionId` instead of `NodeKey`s in expression_scopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Definition {
    // For the import cases, we don't need reference to any arbitrary AST subtrees (annotations,
    // RHS), and referencing just the import statement node is imprecise (a single import statement
    // can assign many symbols, we'd have to re-search for the one we care about), so we just copy
    // the small amount of information we need from the AST.
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(LocalClassId),
    FunctionDef(LocalFunctionId),
    Assignment(LocalAssignmentId),
    AnnotatedAssignment(LocalAnnotatedAssignmentId),
    // NamedExpr(TypedNodeKey<ast::ExprNamed>),
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

// Ideally, `Definition` would only store a local id because the combination of symbol.scope +
// the definition ID is unique.
// trait LocalDefinitionId {
//  type Node;
//  fn ast_id(db: &dyn Db, node: &Self::Node, file: VfsFile, scope: ScopeId) -> Self;
//  fn lookup(self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> AstNodeRef<Self::Node>;
// }

// trait AstId {
//   type Id: LocalDefinitionId;
//
//
//
// }

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
