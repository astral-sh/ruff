use rustc_hash::FxHashMap;
use std::ops::Deref;
use std::sync::Arc;

use ruff_db::parsed::ParsedModule;
use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::{AnyNodeRef, NodeKind};
use ruff_text_size::{Ranged, TextRange};

use crate::red_knot::symbol_table::{global_scopes, semantic_index, GlobalScope, ScopeId};
use crate::Db;

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(no_eq)]
pub fn ast_ids(db: &dyn Db, scope: GlobalScope) -> Arc<AstIds> {
    let index = semantic_index(db, scope.file(db));
    index.ast_ids(scope.scope_id(db))
}

pub struct AstIds {
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

impl PartialEq for AstIds {
    fn eq(&self, other: &Self) -> bool {
        self.expressions == other.expressions && self.statements == other.statements
    }
}

impl Eq for AstIds {}

#[newtype_index]
pub struct LocalExpressionId;

/// ID that uniquely identifies an expression inside a module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ExpressionId {
    scope: ScopeId,
    local: LocalExpressionId,
}

#[newtype_index]
pub struct LocalStatementId;

/// ID that uniquely identifies an expression inside a module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StatementId {
    scope: ScopeId,
    local: LocalStatementId,
}

pub trait LocalAstIdNode {
    type Id;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id;

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self>
    where
        Self: Sized;
}

pub struct AstId<L> {
    scope: ScopeId,
    local: L,
}

pub trait AstIdNode {
    type LocalId;

    /// Resolves the AST id of the node.
    ///
    /// ## Panics
    /// May panic if the node does not belong to the AST of `file` or it returns an incorrect node.

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

impl LocalAstIdNode for ast::Expr {
    type Id = LocalExpressionId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.expressions_map[&NodeKey::from_node(self)]
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let scopes = global_scopes(db, file);
        let global_scope = scopes[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.expressions[id].clone()
    }
}

impl LocalAstIdNode for ast::Stmt {
    type Id = LocalStatementId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        ast_ids.statement_id(self)
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let scopes = global_scopes(db, file);
        let global_scope = scopes[scope];
        let ast_ids = ast_ids(db, global_scope);

        ast_ids.statements[id].clone()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalFunctionId(pub(super) LocalStatementId);

impl LocalAstIdNode for ast::StmtFunctionDef {
    type Id = LocalFunctionId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
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
pub struct LocalClassId(pub(super) LocalStatementId);

impl LocalAstIdNode for ast::StmtClassDef {
    type Id = LocalClassId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
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
        let global_scope = global_scopes(db, file)[scope];
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
        let global_scope = global_scopes(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalAnnotatedAssignmentId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_ann_assign().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalImportId(pub(super) LocalStatementId);

impl LocalAstIdNode for ast::StmtImport {
    type Id = LocalImportId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalImportId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_import().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct LocalImportFromId(pub(super) LocalStatementId);

impl LocalAstIdNode for ast::StmtImportFrom {
    type Id = LocalImportFromId;

    fn local_ast_id(&self, db: &dyn Db, file: VfsFile, scope: ScopeId) -> Self::Id {
        let global_scope = global_scopes(db, file)[scope];
        let ast_ids = ast_ids(db, global_scope);
        LocalImportFromId(ast_ids.statement_id(self))
    }

    fn lookup_local(db: &dyn Db, file: VfsFile, scope: ScopeId, id: Self::Id) -> AstNodeRef<Self> {
        let statement = ast::Stmt::lookup_local(db, file, scope, id.0);
        statement.to_import_from().unwrap()
    }
}

#[derive(Debug)]
pub(super) struct AstIdsBuilder<'a> {
    parsed: &'a ParsedModule,
    expressions: IndexVec<LocalExpressionId, AstNodeRef<ast::Expr>>,
    expressions_map: FxHashMap<NodeKey, LocalExpressionId>,
    statements: IndexVec<LocalStatementId, AstNodeRef<ast::Stmt>>,
    statements_map: FxHashMap<NodeKey, LocalStatementId>,
}

impl<'a> AstIdsBuilder<'a> {
    pub(super) fn new(parsed: &'a ParsedModule) -> Self {
        Self {
            parsed,
            expressions: IndexVec::default(),
            expressions_map: FxHashMap::default(),
            statements: IndexVec::default(),
            statements_map: FxHashMap::default(),
        }
    }

    pub(super) fn record_statement(&mut self, stmt: &ast::Stmt) -> LocalStatementId {
        let statement_id = self
            .statements
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), stmt) });

        self.statements_map
            .insert(NodeKey::from_node(stmt), statement_id);

        statement_id
    }

    pub(super) fn record_expression(&mut self, expr: &ast::Expr) -> LocalExpressionId {
        let expression_id = self
            .expressions
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), expr) });

        self.expressions_map
            .insert(NodeKey::from_node(expr), expression_id);

        expression_id
    }

    pub(super) fn finish(mut self) -> AstIds {
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

#[derive(Clone)]
pub struct AstNodeRef<T> {
    _parsed: ParsedModule,
    node: std::ptr::NonNull<T>,
}

impl<T> AstNodeRef<T> {
    /// Creates a new `AstNodeRef` that reference `node`. The `parsed` is the parsed module to which
    /// the `AstNodeRef` belongs.
    ///
    /// The function is marked as `unsafe` because it is the caller's responsibility to ensure that
    /// `node` is part of `parsed`'s AST. Dereferencing the value if that's not the case is UB
    /// because it can then point to a no longer valid memory location.
    #[allow(unsafe_code)]
    unsafe fn new(parsed: ParsedModule, node: &T) -> Self {
        Self {
            _parsed: parsed,
            node: std::ptr::NonNull::from(node),
        }
    }

    pub fn node(&self) -> &T {
        // SAFETY: Holding on to `parsed` ensures that the AST to which `node` belongs is still alive
        // and not moved.
        #[allow(unsafe_code)]
        unsafe {
            self.node.as_ref()
        }
    }
}

impl<T> Deref for AstNodeRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.node()
    }
}

impl AstNodeRef<ast::Stmt> {
    fn to_class_def(&self) -> Option<AstNodeRef<ast::StmtClassDef>> {
        self.node()
            .as_class_def_stmt()
            .map(|class_def| unsafe { AstNodeRef::new(self._parsed.clone(), class_def) })
    }

    fn to_function_def(&self) -> Option<AstNodeRef<ast::StmtFunctionDef>> {
        self.node()
            .as_function_def_stmt()
            .map(|function_def| unsafe { AstNodeRef::new(self._parsed.clone(), function_def) })
    }

    fn to_assign(self) -> Option<AstNodeRef<ast::StmtAssign>> {
        self.node()
            .as_assign_stmt()
            .map(|assign| unsafe { AstNodeRef::new(self._parsed.clone(), assign) })
    }

    fn to_ann_assign(self) -> Option<AstNodeRef<ast::StmtAnnAssign>> {
        self.node()
            .as_ann_assign_stmt()
            .map(|assign| unsafe { AstNodeRef::new(self._parsed.clone(), assign) })
    }

    fn to_import(self) -> Option<AstNodeRef<ast::StmtImport>> {
        self.node()
            .as_import_stmt()
            .map(|import| unsafe { AstNodeRef::new(self._parsed.clone(), import) })
    }

    fn to_import_from(self) -> Option<AstNodeRef<ast::StmtImportFrom>> {
        self.node()
            .as_import_from_stmt()
            .map(|import| unsafe { AstNodeRef::new(self._parsed.clone(), import) })
    }
}

impl<T> std::fmt::Debug for AstNodeRef<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AstNodeRef").field(&self.node()).finish()
    }
}

impl<T> PartialEq for AstNodeRef<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.node() == other.node()
    }
}

impl<T> Eq for AstNodeRef<T> where T: Eq {}

#[allow(unsafe_code)]
unsafe impl<T> Send for AstNodeRef<T> where T: Send {}
#[allow(unsafe_code)]
unsafe impl<T> Sync for AstNodeRef<T> where T: Sync {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct NodeKey {
    kind: NodeKind,
    range: TextRange,
}

impl NodeKey {
    pub fn from_node<'a, N>(node: N) -> Self
    where
        N: Into<AnyNodeRef<'a>>,
    {
        let node = node.into();
        NodeKey {
            kind: node.kind(),
            range: node.range(),
        }
    }
}
