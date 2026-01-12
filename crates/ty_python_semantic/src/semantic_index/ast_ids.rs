use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;

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
    /// Maps potential synthesized-type call expressions to a call id for stable identity.
    tracked_calls_map: FxHashMap<ExpressionNodeKey, ScopedCallId>,
    /// Stores the ranges of tracked calls, indexed by their [`ScopedCallId`].
    /// Used for diagnostics (e.g., `header_range`).
    tracked_call_ranges: IndexVec<ScopedCallId, TextRange>,
}

impl AstIds {
    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.uses_map[&key.into()]
    }

    /// Returns the call ID for a potential synthesized-type call, if it was tracked during semantic indexing.
    pub(crate) fn try_call_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedCallId> {
        self.tracked_calls_map.get(&key.into()).copied()
    }

    /// Returns the range of a tracked call by its ID.
    pub(crate) fn call_range(&self, id: ScopedCallId) -> TextRange {
        self.tracked_call_ranges[id]
    }
}

fn ast_ids<'db>(db: &'db dyn Db, scope: ScopeId) -> &'db AstIds {
    semantic_index(db, scope.file(db)).ast_ids(scope.file_scope_id(db))
}

/// Uniquely identifies a use of a name in a [`crate::semantic_index::FileScopeId`].
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct ScopedUseId;

/// Uniquely identifies a potential synthesized-type call in a [`crate::semantic_index::FileScopeId`].
///
/// This is used to provide stable identity for inline calls that create synthesized types,
/// such as `type()`, `NamedTuple()`, `TypedDict()`, etc. The ID is assigned during semantic
/// indexing for calls that match known patterns for these synthesizers.
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct ScopedCallId;

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
    tracked_calls_map: FxHashMap<ExpressionNodeKey, ScopedCallId>,
    tracked_call_ranges: IndexVec<ScopedCallId, TextRange>,
}

impl AstIdsBuilder {
    /// Adds `expr` to the use ids map and returns its id.
    pub(super) fn record_use(&mut self, expr: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        let use_id = self.uses_map.len().into();

        self.uses_map.insert(expr.into(), use_id);

        use_id
    }

    /// Records a potential synthesized-type call for stable identity tracking.
    pub(super) fn record_call(
        &mut self,
        expr: impl Into<ExpressionNodeKey>,
        range: TextRange,
    ) -> ScopedCallId {
        let call_id = self.tracked_call_ranges.push(range);
        self.tracked_calls_map.insert(expr.into(), call_id);
        call_id
    }

    pub(super) fn finish(mut self) -> AstIds {
        self.uses_map.shrink_to_fit();
        self.tracked_calls_map.shrink_to_fit();

        AstIds {
            uses_map: self.uses_map,
            tracked_calls_map: self.tracked_calls_map,
            tracked_call_ranges: self.tracked_call_ranges,
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
