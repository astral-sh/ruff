use rustc_hash::FxHashMap;
use std::sync::Arc;

use ruff_db::parsed::ParsedModule;
use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::AnyNodeRef;

use crate::red_knot::ast_node_ref::AstNodeRef;
use crate::red_knot::node_key::NodeKey;
use crate::red_knot::semantic_index::{scopes_map, semantic_index, GlobalScope, ScopeId};
use crate::Db;

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(no_eq)]
pub fn ast_ids(db: &dyn Db, scope: GlobalScope) -> Arc<AstIds> {
    let index = semantic_index(db, scope.file(db));
    index.ast_ids(scope.scope_id(db))
}

/// AST ids for a single scope.
///
/// The motivation for building the AST ids per scope isn't about reducing invalidation because
/// the struct changes whenever the parsed AST changes. Instead, it's mainly that we can
/// build the AST ids struct when building the symbol table and also keep the property that
/// IDs of outer scopes are unaffected by changes in inner scopes.
///
/// For example, we don't want that adding new statements to `foo` changes the statement id of `x = foo()` in:
///
/// ```python
/// def foo():
///     return 5
///
/// x = foo()
/// ```
pub struct AstIds {
    /// Maps expression ids to their expressions.
    expressions: IndexVec<LocalExpressionId, AstNodeRef<ast::Expr>>,

    /// Maps expressions to their expression id. Uses `NodeKey` because it avoids cloning [`Parsed`].
    expressions_map: FxHashMap<NodeKey, LocalExpressionId>,

    statements: IndexVec<LocalStatementId, AstNodeRef<ast::Stmt>>,

    statements_map: FxHashMap<NodeKey, LocalStatementId>,
}

impl AstIds {
    fn statement_id<'a, N>(&self, node: N) -> LocalStatementId
    where
        N: Into<AnyNodeRef<'a>>,
    {
        self.statements_map[&NodeKey::from_node(node.into())]
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for AstIds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AstIds")
            .field("expressions", &self.expressions)
            .field("statements", &self.statements)
            .finish()
    }
}

/// Uniquely identifies an [`ast::Expr`] in a [`ScopeId`].
#[newtype_index]
pub struct LocalExpressionId;

/// Uniquely identifies an [`ast::Stmt`] in a [`ScopeId`].
#[newtype_index]
pub struct LocalStatementId;

/// Node that can be uniquely identified by an id in a [`ScopeId`].
pub trait LocalAstIdNode {
    /// The type of the ID uniquely identifying the node.
    type Id;

    /// Returns the ID that uniquely identifies the node in `scope`.
    ///
    /// ## Panics
    /// Panics if the node doesn't belong to `file` or is outside `scope`.
    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id;

    /// Looks up the AST node by its ID.
    ///
    /// ## Panics
    /// May panic if the `id` does not belong to the AST of `file`, or is outside `scope`.
    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self>
    where
        Self: Sized;
}

/// Extension trait for AST nodes that can be resolved by an `AstId`.
pub trait AstIdNode {
    type LocalId;

    /// Resolves the AST id of the node.
    ///
    /// ## Panics
    /// May panic if the node does not belongs to `file`'s AST or is outside of `scope`. It may also
    /// return an incorrect node if that's the case.

    fn ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> AstId<Self::LocalId>;

    /// Resolves the AST node for `id`.
    ///
    /// ## Panics
    /// May panic if the `id` does not belong to the AST of `file` or it returns an incorrect node.

    fn lookup(db: &dyn Db, file: VfsFile, id: AstId<Self::LocalId>) -> AstNodeRef<Self>
    where
        Self: Sized;
}

impl<T> AstIdNode for T
where
    T: LocalAstIdNode,
{
    type LocalId = T::Id;

    fn ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> AstId<Self::LocalId> {
        let local_id = self.local_ast_id(db, file, scope);
        AstId {
            scope,
            local: local_id,
        }
    }

    fn lookup(db: &dyn Db, file: VfsFile, id: AstId<Self::LocalId>) -> AstNodeRef<Self>
    where
        Self: Sized,
    {
        let scope = id.scope;
        Self::lookup_local(db, file, scope, id.local)
    }
}

/// Uniquely identifies an AST node in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AstId<L> {
    /// The node's scope.
    scope: ScopeId,

    /// The ID of the node inside [`Self::scope`].
    local: L,
}

impl LocalAstIdNode for ast::Expr {
    type Id = LocalExpressionId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.expressions_map[&NodeKey::from_node(self)]
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let scopes = scopes_map(db, file);
        let global_scope = scopes[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.expressions[id].clone()
    }
}

impl LocalAstIdNode for ast::Stmt {
    type Id = LocalStatementId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.statement_id(self)
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let scopes = scopes_map(db, file);
        let global_scope = scopes[scope];
        let ast_ids = ast_ids(db, global_scope);

        ast_ids.statements[id].clone()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalFunctionId(pub(in crate::red_knot) LocalStatementId);

impl LocalAstIdNode for ast::StmtFunctionDef {
    type Id = LocalFunctionId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalFunctionId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        ast::Stmt::lookup_local(db, file, scope, id.0)
            .to_function_def()
            .unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalClassId(pub(in crate::red_knot) LocalStatementId);

impl LocalAstIdNode for ast::StmtClassDef {
    type Id = LocalClassId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalClassId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_class_def().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalAssignmentId(LocalStatementId);

impl LocalAstIdNode for ast::StmtAssign {
    type Id = LocalAssignmentId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalAssignmentId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_assign().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalAnnotatedAssignmentId(LocalStatementId);

impl LocalAstIdNode for ast::StmtAnnAssign {
    type Id = LocalAnnotatedAssignmentId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalAnnotatedAssignmentId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_ann_assign().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalImportId(pub(in crate::red_knot) LocalStatementId);

impl LocalAstIdNode for ast::StmtImport {
    type Id = LocalImportId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalImportId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_import().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalImportFromId(pub(in crate::red_knot) LocalStatementId);

impl LocalAstIdNode for ast::StmtImportFrom {
    type Id = LocalImportFromId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = scopes_map(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalImportFromId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_import_from().unwrap()
    }
}

#[derive(Debug)]
pub(in crate::red_knot) struct AstIdsBuilder<'a> {
    parsed: &'a ParsedModule,
    expressions: IndexVec<LocalExpressionId, AstNodeRef<ast::Expr>>,
    expressions_map: FxHashMap<NodeKey, LocalExpressionId>,
    statements: IndexVec<LocalStatementId, AstNodeRef<ast::Stmt>>,
    statements_map: FxHashMap<NodeKey, LocalStatementId>,
}

impl<'a> AstIdsBuilder<'a> {
    pub(in crate::red_knot) fn new(parsed: &'a ParsedModule) -> Self {
        Self {
            parsed,
            expressions: IndexVec::default(),
            expressions_map: FxHashMap::default(),
            statements: IndexVec::default(),
            statements_map: FxHashMap::default(),
        }
    }

    pub(in crate::red_knot) fn record_statement(&mut self, stmt: &ast::Stmt) -> LocalStatementId {
        let statement_id = self
            .statements
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), stmt) });

        self.statements_map
            .insert(NodeKey::from_node(stmt), statement_id);

        statement_id
    }

    pub(in crate::red_knot) fn record_expression(&mut self, expr: &ast::Expr) -> LocalExpressionId {
        let expression_id = self
            .expressions
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), expr) });

        self.expressions_map
            .insert(NodeKey::from_node(expr), expression_id);

        expression_id
    }

    pub(in crate::red_knot) fn finish(mut self) -> AstIds {
        self.expressions.shrink_to_fit();
        self.expressions_map.shrink_to_fit();
        self.statements.shrink_to_fit();
        self.statements_map.shrink_to_fit();

        AstIds {
            expressions: self.expressions,
            expressions_map: self.expressions_map,
            statements: self.statements,
            statements_map: self.statements_map,
        }
    }
}
