use rustc_hash::FxHashMap;

use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::Db;
use crate::environment::AnalysisFile;
use crate::frozen::FrozenMap;
use crate::scope::FileScopeId;
use crate::semantic_index;

pub use node_key::ExpressionNodeKey;

/// AST ids for a file.
///
/// Use IDs are assigned per scope while building the semantic index. This keeps the property that
/// IDs of outer scopes are unaffected by changes in inner scopes. Node IDs are unique within a
/// file, so the final reverse lookup can merge the per-scope maps into a single map.
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
    uses_map: FrozenMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIds {
    pub(super) fn from_builders(builders: IndexVec<FileScopeId, AstIdsBuilder>) -> Self {
        let capacity = builders.iter().map(|builder| builder.uses_map.len()).sum();
        let mut uses_map = Vec::with_capacity(capacity);

        for builder in builders {
            uses_map.extend(builder.uses_map);
        }

        let uses_map = FrozenMap::from_entries(uses_map);
        debug_assert!(
            uses_map.keys().is_sorted_by(|left, right| left < right),
            "AST ID builders must contain disjoint keys"
        );

        Self { uses_map }
    }

    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.uses_map[&key.into()]
    }
}

fn ast_ids<'db>(db: &'db dyn Db, file: AnalysisFile<'db>) -> &'db AstIds {
    semantic_index(db, file).ast_ids()
}

/// Uniquely identifies a use of a name in a [`crate::FileScopeId`].
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedUseId;

pub trait HasScopedUseId {
    /// Returns the ID that uniquely identifies the use in its scope.
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId;
}

impl HasScopedUseId for ast::Identifier {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
        ast_ids.use_id(self)
    }
}

impl HasScopedUseId for ast::ExprName {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::ExprAttribute {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::ExprSubscript {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::Keyword {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
        ast_ids.use_id(self)
    }
}

impl HasScopedUseId for ast::ExprRef<'_> {
    fn scoped_use_id(&self, db: &dyn Db, file: AnalysisFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
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

    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        self.uses_map.get(&key.into()).copied()
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast as ast;

    use crate::{ast_node_ref::AstNodeRef, node_key::NodeKey};

    #[derive(
        Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, salsa::Update, get_size2::GetSize,
    )]
    pub struct ExpressionNodeKey(NodeKey);

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

    impl From<&Box<ast::Expr>> for ExpressionNodeKey {
        fn from(value: &Box<ast::Expr>) -> Self {
            Self(NodeKey::from_node(&**value))
        }
    }

    impl From<&ast::ExprCall> for ExpressionNodeKey {
        fn from(value: &ast::ExprCall) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::ExprLambda> for ExpressionNodeKey {
        fn from(value: &ast::ExprLambda) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::Identifier> for ExpressionNodeKey {
        fn from(value: &ast::Identifier) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl From<&ast::Keyword> for ExpressionNodeKey {
        fn from(value: &ast::Keyword) -> Self {
            Self(NodeKey::from_node(value))
        }
    }

    impl<T> From<&AstNodeRef<T>> for ExpressionNodeKey {
        fn from(value: &AstNodeRef<T>) -> Self {
            Self(NodeKey::from_node_ref(value))
        }
    }
}
