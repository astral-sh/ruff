use rustc_hash::FxHashMap;

use ruff_index::newtype_index;
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::Db;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::semantic_index;

/// AST ids for a single scope.
///
/// The motivation for building the AST ids per scope isn't about reducing invalidation because
/// the struct changes whenever the parsed AST changes. Instead, it's mainly that we can
/// build the AST ids struct when building the place table and also keep the property that
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
#[derive(Debug, salsa::Update, get_size2::GetSize)]
pub(crate) struct AstIds {
    /// Maps expressions which "use" a place (that is, [`ast::ExprName`], [`ast::ExprAttribute`] or [`ast::ExprSubscript`]) to a use id.
    uses_map: FxHashMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIds {
    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.uses_map[&key.into()]
    }
}

fn ast_ids<'db>(db: &'db dyn Db, scope: ScopeId) -> &'db AstIds {
    semantic_index(db, scope.file(db)).ast_ids(scope.file_scope_id(db))
}

/// Uniquely identifies a use of a name in a [`crate::semantic_index::FileScopeId`].
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct ScopedUseId;

pub trait HasScopedUseId {
    /// Returns the ID that uniquely identifies the use in `scope`.
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId;
}

impl HasScopedUseId for ast::Identifier {
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId {
        let ast_ids = ast_ids(db, scope);
        ast_ids.use_id(self)
    }
}

impl HasScopedUseId for ast::ExprName {
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, scope)
    }
}

impl HasScopedUseId for ast::ExprAttribute {
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, scope)
    }
}

impl HasScopedUseId for ast::ExprSubscript {
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, scope)
    }
}

impl HasScopedUseId for ast::ExprRef<'_> {
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId {
        let ast_ids = ast_ids(db, scope);
        ast_ids.use_id(*self)
    }
}

#[derive(Debug, Default)]
pub(super) struct AstIdsBuilder {
    uses_map: FxHashMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIdsBuilder {
    /// Adds `expr` to the use ids map and returns its id.
    pub(super) fn record_use(&mut self, expr: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        let use_id = self.uses_map.len().into();

        self.uses_map.insert(expr.into(), use_id);

        use_id
    }

    pub(super) fn finish(mut self) -> AstIds {
        self.uses_map.shrink_to_fit();

        AstIds {
            uses_map: self.uses_map,
        }
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast as ast;

    use crate::node_key::NodeKey;

    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, salsa::Update, get_size2::GetSize)]
    pub(crate) struct ExpressionNodeKey(NodeKey);

    impl From<ast::ExprRef<'_>> for ExpressionNodeKey {
        fn from(value: ast::ExprRef<'_>) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::Expr> for ExpressionNodeKey {
        fn from(value: &ast::Expr) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::ExprCall> for ExpressionNodeKey {
        fn from(value: &ast::ExprCall) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::Identifier> for ExpressionNodeKey {
        fn from(value: &ast::Identifier) -> Self {
            Self(NodeKey::from_node(value))
        }
    }
}
