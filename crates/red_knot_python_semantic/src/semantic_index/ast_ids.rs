use rustc_hash::FxHashMap;

use ruff_db::parsed::ParsedModule;
use ruff_db::vfs::VfsFile;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::{AnyNodeRef, ExpressionRef};

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::{FileScopeId, ScopeId};
use crate::Db;

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
pub(crate) struct AstIds {
    /// Maps expression ids to their expressions.
    expressions: IndexVec<ScopedExpressionId, AstNodeRef<ast::Expr>>,

    /// Maps expressions to their expression id. Uses `NodeKey` because it avoids cloning [`Parsed`].
    expressions_map: FxHashMap<NodeKey, ScopedExpressionId>,

    statements: IndexVec<ScopedStatementId, AstNodeRef<ast::Stmt>>,

    statements_map: FxHashMap<NodeKey, ScopedStatementId>,
}

impl AstIds {
    fn statement_id<'a, N>(&self, node: N) -> ScopedStatementId
    where
        N: Into<AnyNodeRef<'a>>,
    {
        self.statements_map[&NodeKey::from_node(node.into())]
    }

    fn expression_id<'a, N>(&self, node: N) -> ScopedExpressionId
    where
        N: Into<ExpressionRef<'a>>,
    {
        self.expressions_map[&NodeKey::from_node(node.into())]
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

fn ast_ids<'db>(db: &'db dyn Db, scope: ScopeId) -> &'db AstIds {
    semantic_index(db, scope.file(db)).ast_ids(scope.file_scope_id(db))
}

pub trait HasScopedAstId {
    /// The type of the ID uniquely identifying the node.
    type Id: Copy;

    /// Returns the ID that uniquely identifies the node in `scope`.
    ///
    /// ## Panics
    /// Panics if the node doesn't belong to `file` or is outside `scope`.
    fn scoped_ast_id(&self, db: &dyn Db, file: VfsFile, scope: FileScopeId) -> Self::Id;
}

/// Node that can be uniquely identified by an id in a [`FileScopeId`].
pub trait ScopedAstIdNode: HasScopedAstId {
    /// Looks up the AST node by its ID.
    ///
    /// ## Panics
    /// May panic if the `id` does not belong to the AST of `file`, or is outside `scope`.
    fn lookup_in_scope(db: &dyn Db, file: VfsFile, scope: FileScopeId, id: Self::Id) -> &Self
    where
        Self: Sized;
}

/// Extension trait for AST nodes that can be resolved by an `AstId`.
pub trait AstIdNode {
    type ScopeId: Copy;

    /// Resolves the AST id of the node.
    ///
    /// ## Panics
    /// May panic if the node does not belongs to `file`'s AST or is outside of `scope`. It may also
    /// return an incorrect node if that's the case.
    fn ast_id(&self, db: &dyn Db, file: VfsFile, scope: FileScopeId) -> AstId<Self::ScopeId>;

    /// Resolves the AST node for `id`.
    ///
    /// ## Panics
    /// May panic if the `id` does not belong to the AST of `file` or it returns an incorrect node.

    fn lookup(db: &dyn Db, file: VfsFile, id: AstId<Self::ScopeId>) -> &Self
    where
        Self: Sized;
}

impl<T> AstIdNode for T
where
    T: ScopedAstIdNode,
{
    type ScopeId = T::Id;

    fn ast_id(&self, db: &dyn Db, file: VfsFile, scope: FileScopeId) -> AstId<Self::ScopeId> {
        let in_scope_id = self.scoped_ast_id(db, file, scope);
        AstId { scope, in_scope_id }
    }

    fn lookup(db: &dyn Db, file: VfsFile, id: AstId<Self::ScopeId>) -> &Self
    where
        Self: Sized,
    {
        let scope = id.scope;
        Self::lookup_in_scope(db, file, scope, id.in_scope_id)
    }
}

/// Uniquely identifies an AST node in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AstId<L: Copy> {
    /// The node's scope.
    scope: FileScopeId,

    /// The ID of the node inside [`Self::scope`].
    in_scope_id: L,
}

impl<L: Copy> AstId<L> {
    pub(super) fn new(scope: FileScopeId, in_scope_id: L) -> Self {
        Self { scope, in_scope_id }
    }

    pub(super) fn in_scope_id(self) -> L {
        self.in_scope_id
    }
}

/// Uniquely identifies an [`ast::Expr`] in a [`FileScopeId`].
#[newtype_index]
pub struct ScopedExpressionId;

macro_rules! impl_has_scoped_expression_id {
    ($ty: ty) => {
        impl HasScopedAstId for $ty {
            type Id = ScopedExpressionId;

            fn scoped_ast_id(
                &self,
                db: &dyn Db,
                file: VfsFile,
                file_scope: FileScopeId,
            ) -> Self::Id {
                let expression_ref = ExpressionRef::from(self);
                expression_ref.scoped_ast_id(db, file, file_scope)
            }
        }
    };
}

impl_has_scoped_expression_id!(ast::ExprBoolOp);
impl_has_scoped_expression_id!(ast::ExprName);
impl_has_scoped_expression_id!(ast::ExprBinOp);
impl_has_scoped_expression_id!(ast::ExprUnaryOp);
impl_has_scoped_expression_id!(ast::ExprLambda);
impl_has_scoped_expression_id!(ast::ExprIf);
impl_has_scoped_expression_id!(ast::ExprDict);
impl_has_scoped_expression_id!(ast::ExprSet);
impl_has_scoped_expression_id!(ast::ExprListComp);
impl_has_scoped_expression_id!(ast::ExprSetComp);
impl_has_scoped_expression_id!(ast::ExprDictComp);
impl_has_scoped_expression_id!(ast::ExprGenerator);
impl_has_scoped_expression_id!(ast::ExprAwait);
impl_has_scoped_expression_id!(ast::ExprYield);
impl_has_scoped_expression_id!(ast::ExprYieldFrom);
impl_has_scoped_expression_id!(ast::ExprCompare);
impl_has_scoped_expression_id!(ast::ExprCall);
impl_has_scoped_expression_id!(ast::ExprFString);
impl_has_scoped_expression_id!(ast::ExprStringLiteral);
impl_has_scoped_expression_id!(ast::ExprBytesLiteral);
impl_has_scoped_expression_id!(ast::ExprNumberLiteral);
impl_has_scoped_expression_id!(ast::ExprBooleanLiteral);
impl_has_scoped_expression_id!(ast::ExprNoneLiteral);
impl_has_scoped_expression_id!(ast::ExprEllipsisLiteral);
impl_has_scoped_expression_id!(ast::ExprAttribute);
impl_has_scoped_expression_id!(ast::ExprSubscript);
impl_has_scoped_expression_id!(ast::ExprStarred);
impl_has_scoped_expression_id!(ast::ExprNamed);
impl_has_scoped_expression_id!(ast::ExprList);
impl_has_scoped_expression_id!(ast::ExprTuple);
impl_has_scoped_expression_id!(ast::ExprSlice);
impl_has_scoped_expression_id!(ast::ExprIpyEscapeCommand);
impl_has_scoped_expression_id!(ast::Expr);

impl HasScopedAstId for ast::ExpressionRef<'_> {
    type Id = ScopedExpressionId;

    fn scoped_ast_id(&self, db: &dyn Db, file: VfsFile, file_scope: FileScopeId) -> Self::Id {
        let scope = file_scope.to_scope_id(db, file);
        let ast_ids = ast_ids(db, scope);
        ast_ids.expression_id(*self)
    }
}

impl ScopedAstIdNode for ast::Expr {
    fn lookup_in_scope(db: &dyn Db, file: VfsFile, file_scope: FileScopeId, id: Self::Id) -> &Self {
        let scope = file_scope.to_scope_id(db, file);
        let ast_ids = ast_ids(db, scope);
        ast_ids.expressions[id].node()
    }
}

/// Uniquely identifies an [`ast::Stmt`] in a [`FileScopeId`].
#[newtype_index]
pub struct ScopedStatementId;

macro_rules! impl_has_scoped_statement_id {
    ($ty: ty) => {
        impl HasScopedAstId for $ty {
            type Id = ScopedStatementId;

            fn scoped_ast_id(
                &self,
                db: &dyn Db,
                file: VfsFile,
                file_scope: FileScopeId,
            ) -> Self::Id {
                let scope = file_scope.to_scope_id(db, file);
                let ast_ids = ast_ids(db, scope);
                ast_ids.statement_id(self)
            }
        }
    };
}

impl_has_scoped_statement_id!(ast::Stmt);

impl ScopedAstIdNode for ast::Stmt {
    fn lookup_in_scope(db: &dyn Db, file: VfsFile, file_scope: FileScopeId, id: Self::Id) -> &Self {
        let scope = file_scope.to_scope_id(db, file);
        let ast_ids = ast_ids(db, scope);

        ast_ids.statements[id].node()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ScopedFunctionId(pub(super) ScopedStatementId);

impl HasScopedAstId for ast::StmtFunctionDef {
    type Id = ScopedFunctionId;

    fn scoped_ast_id(&self, db: &dyn Db, file: VfsFile, file_scope: FileScopeId) -> Self::Id {
        let scope = file_scope.to_scope_id(db, file);
        let ast_ids = ast_ids(db, scope);
        ScopedFunctionId(ast_ids.statement_id(self))
    }
}

impl ScopedAstIdNode for ast::StmtFunctionDef {
    fn lookup_in_scope(db: &dyn Db, file: VfsFile, scope: FileScopeId, id: Self::Id) -> &Self {
        ast::Stmt::lookup_in_scope(db, file, scope, id.0)
            .as_function_def_stmt()
            .unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ScopedClassId(pub(super) ScopedStatementId);

impl HasScopedAstId for ast::StmtClassDef {
    type Id = ScopedClassId;

    fn scoped_ast_id(&self, db: &dyn Db, file: VfsFile, file_scope: FileScopeId) -> Self::Id {
        let scope = file_scope.to_scope_id(db, file);
        let ast_ids = ast_ids(db, scope);
        ScopedClassId(ast_ids.statement_id(self))
    }
}

impl ScopedAstIdNode for ast::StmtClassDef {
    fn lookup_in_scope(db: &dyn Db, file: VfsFile, scope: FileScopeId, id: Self::Id) -> &Self {
        let statement = ast::Stmt::lookup_in_scope(db, file, scope, id.0);
        statement.as_class_def_stmt().unwrap()
    }
}

impl_has_scoped_statement_id!(ast::StmtAssign);
impl_has_scoped_statement_id!(ast::StmtAnnAssign);
impl_has_scoped_statement_id!(ast::StmtImport);
impl_has_scoped_statement_id!(ast::StmtImportFrom);

#[derive(Debug)]
pub(super) struct AstIdsBuilder {
    expressions: IndexVec<ScopedExpressionId, AstNodeRef<ast::Expr>>,
    expressions_map: FxHashMap<NodeKey, ScopedExpressionId>,
    statements: IndexVec<ScopedStatementId, AstNodeRef<ast::Stmt>>,
    statements_map: FxHashMap<NodeKey, ScopedStatementId>,
}

impl AstIdsBuilder {
    pub(super) fn new() -> Self {
        Self {
            expressions: IndexVec::default(),
            expressions_map: FxHashMap::default(),
            statements: IndexVec::default(),
            statements_map: FxHashMap::default(),
        }
    }

    /// Adds `stmt` to the AST ids map and returns its id.
    ///
    /// ## Safety
    /// The function is marked as unsafe because it calls [`AstNodeRef::new`] which requires
    /// that `stmt` is a child of `parsed`.
    #[allow(unsafe_code)]
    pub(super) unsafe fn record_statement(
        &mut self,
        stmt: &ast::Stmt,
        parsed: &ParsedModule,
    ) -> ScopedStatementId {
        let statement_id = self.statements.push(AstNodeRef::new(parsed.clone(), stmt));

        self.statements_map
            .insert(NodeKey::from_node(stmt), statement_id);

        statement_id
    }

    /// Adds `expr` to the AST ids map and returns its id.
    ///
    /// ## Safety
    /// The function is marked as unsafe because it calls [`AstNodeRef::new`] which requires
    /// that `expr` is a child of `parsed`.
    #[allow(unsafe_code)]
    pub(super) unsafe fn record_expression(
        &mut self,
        expr: &ast::Expr,
        parsed: &ParsedModule,
    ) -> ScopedExpressionId {
        let expression_id = self.expressions.push(AstNodeRef::new(parsed.clone(), expr));

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
