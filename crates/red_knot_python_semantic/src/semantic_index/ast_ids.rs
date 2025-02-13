use rustc_hash::FxHashMap;

use ruff_index::newtype_index;
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::ScopeId;
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
#[derive(Debug, salsa::Update)]
pub(crate) struct AstIds {
    /// Maps expressions to their expression id.
    expressions_map: FxHashMap<ExpressionNodeKey, ScopedExpressionId>,
    /// Maps expressions which "use" a symbol (that is, [`ast::ExprName`]) to a use id.
    uses_map: FxHashMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIds {
    fn expression_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedExpressionId {
        let key = &key.into();
        *self.expressions_map.get(key).unwrap_or_else(|| {
            panic!("Could not find expression ID for {key:?}");
        })
    }

    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.uses_map[&key.into()]
    }
}

fn ast_ids<'db>(db: &'db dyn Db, scope: ScopeId) -> &'db AstIds {
    semantic_index(db, scope.file(db)).ast_ids(scope.file_scope_id(db))
}

/// Uniquely identifies a use of a name in a [`crate::semantic_index::symbol::FileScopeId`].
#[newtype_index]
pub struct ScopedUseId;

pub trait HasScopedUseId {
    /// Returns the ID that uniquely identifies the use in `scope`.
    fn scoped_use_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedUseId;
}

impl HasScopedUseId for ast::ExprName {
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

/// Uniquely identifies an [`ast::Expr`] in a [`crate::semantic_index::symbol::FileScopeId`].
#[newtype_index]
#[derive(salsa::Update)]
pub struct ScopedExpressionId;

pub trait HasScopedExpressionId {
    /// Returns the ID that uniquely identifies the node in `scope`.
    fn scoped_expression_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedExpressionId;
}

impl<T: HasScopedExpressionId> HasScopedExpressionId for Box<T> {
    fn scoped_expression_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedExpressionId {
        self.as_ref().scoped_expression_id(db, scope)
    }
}

macro_rules! impl_has_scoped_expression_id {
    ($ty: ty) => {
        impl HasScopedExpressionId for $ty {
            fn scoped_expression_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedExpressionId {
                let expression_ref = ExprRef::from(self);
                expression_ref.scoped_expression_id(db, scope)
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

impl HasScopedExpressionId for ast::ExprRef<'_> {
    fn scoped_expression_id(&self, db: &dyn Db, scope: ScopeId) -> ScopedExpressionId {
        let ast_ids = ast_ids(db, scope);
        ast_ids.expression_id(*self)
    }
}

#[derive(Debug, Default)]
pub(super) struct AstIdsBuilder {
    expressions_map: FxHashMap<ExpressionNodeKey, ScopedExpressionId>,
    uses_map: FxHashMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIdsBuilder {
    /// Adds `expr` to the expression ids map and returns its id.
    pub(super) fn record_expression(&mut self, expr: &ast::Expr) -> ScopedExpressionId {
        let expression_id = self.expressions_map.len().into();

        self.expressions_map.insert(expr.into(), expression_id);

        expression_id
    }

    /// Adds `expr` to the use ids map and returns its id.
    pub(super) fn record_use(&mut self, expr: &ast::Expr) -> ScopedUseId {
        let use_id = self.uses_map.len().into();

        self.uses_map.insert(expr.into(), use_id);

        use_id
    }

    pub(super) fn finish(mut self) -> AstIds {
        self.expressions_map.shrink_to_fit();
        self.uses_map.shrink_to_fit();

        AstIds {
            expressions_map: self.expressions_map,
            uses_map: self.uses_map,
        }
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast as ast;

    use crate::node_key::NodeKey;

    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, salsa::Update)]
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
}
