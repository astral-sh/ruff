use rustc_hash::FxHashMap;

use ruff_index::newtype_index;
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::node_key::NodeKey;
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
#[derive(Debug)]
pub(crate) struct AstIds {
    /// Maps expressions to their expression id.
    expressions: FxHashMap<ExpressionNodeKey, ScopedExpressionId>,
    /// Maps expressions which "use" a symbol (that is, [`ast::ExprName`]) to a use id.
    uses: FxHashMap<ExpressionNodeKey, ScopedUseId>,
    /// Maps nodes that represent nested scopes to unique IDs representing those scopes.
    eager_nested_scopes: FxHashMap<EagerNestedScopeNodeKey, ScopedEagerNestedScopeId>,
}

impl AstIds {
    fn expression_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedExpressionId {
        let key = &key.into();
        *self.expressions.get(key).unwrap_or_else(|| {
            panic!("Could not find expression ID for {key:?}");
        })
    }

    #[track_caller]
    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.uses[&key.into()]
    }

    #[track_caller]
    fn eager_nested_scope_id(&self, key: EagerNestedScopeNodeKey) -> ScopedEagerNestedScopeId {
        self.eager_nested_scopes[&key]
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

/// Uniquely identifies a nested "eager scope".
///
/// An eager scope has its entire body executed immediately at the location where it is defined.
/// This is required to store a record of snapshots that provide information on the definition
/// states of symbols in the parent scope at the point where the nested scope is defined.
#[newtype_index]
pub(crate) struct ScopedEagerNestedScopeId;

pub(super) trait HasScopedEagerNestedScopeId {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId;
}

// A list comprehension executes its nested scope eagerly;
// it may use symbols from its parent scope at the point where the comprehension
// is *defined* rather than symbols from the end of the parent scope
impl HasScopedEagerNestedScopeId for ast::ExprListComp {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId {
        ast_ids(db, outer_scope).eager_nested_scope_id(self.into())
    }
}

// A dict comprehension executes its nested scope eagerly;
// it may use symbols from its parent scope at the point where the comprehension
// is *defined* rather than symbols from the end of the parent scope
impl HasScopedEagerNestedScopeId for ast::ExprDictComp {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId {
        ast_ids(db, outer_scope).eager_nested_scope_id(self.into())
    }
}

// A set comprehension executes its nested scope eagerly;
// it may use symbols from its parent scope at the point where the comprehension
// is *defined* rather than symbols from the end of the parent scope
impl HasScopedEagerNestedScopeId for ast::ExprSetComp {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId {
        ast_ids(db, outer_scope).eager_nested_scope_id(self.into())
    }
}

// Generator expressions are interesting!
//
// One of the key benefits of a generator expression is that it *can* run lazily,
// as opposed to a list comprehension. However, in real-world uses, a majority of
// generator expressions are executed eagerly, passed to functions such as `any()` and `all()`.
// As such (and matching the behaviour of other type checkers such as mypy/pyright),
// we model generator expressions as always executing their nested scopes eagerly,
// even though this isn't always *strictly* accurate.
//
// Practicality beats purity!
impl HasScopedEagerNestedScopeId for ast::ExprGenerator {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId {
        ast_ids(db, outer_scope).eager_nested_scope_id(self.into())
    }
}

// Class bodies are also eager.
impl HasScopedEagerNestedScopeId for ast::StmtClassDef {
    fn scoped_eager_nested_scope_id(
        &self,
        db: &dyn Db,
        outer_scope: ScopeId,
    ) -> ScopedEagerNestedScopeId {
        ast_ids(db, outer_scope).eager_nested_scope_id(self.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct EagerNestedScopeNodeKey(NodeKey);

impl From<&ast::StmtClassDef> for EagerNestedScopeNodeKey {
    fn from(value: &ast::StmtClassDef) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprListComp> for EagerNestedScopeNodeKey {
    fn from(value: &ast::ExprListComp) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprSetComp> for EagerNestedScopeNodeKey {
    fn from(value: &ast::ExprSetComp) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprDictComp> for EagerNestedScopeNodeKey {
    fn from(value: &ast::ExprDictComp) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprGenerator> for EagerNestedScopeNodeKey {
    fn from(value: &ast::ExprGenerator) -> Self {
        Self(NodeKey::from_node(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum EagerNestedScopeRef<'a> {
    Class(&'a ast::StmtClassDef),
    ListComprehension(&'a ast::ExprListComp),
    SetComprehension(&'a ast::ExprSetComp),
    DictComprehension(&'a ast::ExprDictComp),
    GeneratorExpression(&'a ast::ExprGenerator),
}

impl From<EagerNestedScopeRef<'_>> for EagerNestedScopeNodeKey {
    fn from(value: EagerNestedScopeRef) -> Self {
        match value {
            EagerNestedScopeRef::Class(class) => class.into(),
            EagerNestedScopeRef::DictComprehension(dict_comprehension) => dict_comprehension.into(),
            EagerNestedScopeRef::GeneratorExpression(generator) => generator.into(),
            EagerNestedScopeRef::ListComprehension(list_comprehension) => list_comprehension.into(),
            EagerNestedScopeRef::SetComprehension(set_comprehension) => set_comprehension.into(),
        }
    }
}

/// Uniquely identifies an [`ast::Expr`] in a [`crate::semantic_index::symbol::FileScopeId`].
#[newtype_index]
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
    expressions: FxHashMap<ExpressionNodeKey, ScopedExpressionId>,
    uses: FxHashMap<ExpressionNodeKey, ScopedUseId>,
    eager_nested_scopes: FxHashMap<EagerNestedScopeNodeKey, ScopedEagerNestedScopeId>,
}

impl AstIdsBuilder {
    /// Adds `expr` to the expression ids map and returns its id.
    pub(super) fn record_expression(&mut self, expr: &ast::Expr) -> ScopedExpressionId {
        let expression_id = self.expressions.len().into();

        self.expressions.insert(expr.into(), expression_id);

        expression_id
    }

    /// Adds `expr` to the use ids map and returns its id.
    pub(super) fn record_use(&mut self, expr: &ast::Expr) -> ScopedUseId {
        let use_id = self.uses.len().into();

        self.uses.insert(expr.into(), use_id);

        use_id
    }

    pub(super) fn record_eager_nested_scope(
        &mut self,
        node: impl Into<EagerNestedScopeNodeKey>,
    ) -> ScopedEagerNestedScopeId {
        let nested_scope_id = self.eager_nested_scopes.len().into();

        self.eager_nested_scopes
            .insert(node.into(), nested_scope_id);

        nested_scope_id
    }

    pub(super) fn finish(mut self) -> AstIds {
        self.expressions.shrink_to_fit();
        self.uses.shrink_to_fit();
        self.eager_nested_scopes.shrink_to_fit();

        AstIds {
            expressions: self.expressions,
            uses: self.uses,
            eager_nested_scopes: self.eager_nested_scopes,
        }
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast as ast;

    use crate::node_key::NodeKey;

    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
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
