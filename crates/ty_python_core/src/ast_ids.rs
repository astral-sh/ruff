use std::collections::BTreeMap;

use ruff_index::newtype_index;
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::Db;
use crate::HasTrackedScope;
use crate::ProgramFile;
use crate::place::ScopedPlaceId;
use crate::scope::FileScopeId;
use crate::semantic_index;

pub use node_key::ExpressionNodeKey;

/// The scope and use IDs recorded for one AST node during indexing.
#[derive(Clone, Copy, Default, Debug)]
struct NodeIds {
    scope: Option<FileScopeId>,
    use_id: Option<ScopedUseId>,
}

/// A read of a lexical place in a particular scope.
///
/// ```python
/// def f(x):
///     before = x
///     x = 1
///     return x
/// ```
///
/// Both reads of `x` share a `scope` (the body of `f`) and a `place` (the local variable `x`).
/// Each read has its own `use_id`, which identifies the bindings that can reach it.
/// The read in `before = x` sees the parameter binding, and the read in `return x` sees `x = 1`.
#[derive(Clone, Copy, Debug)]
pub struct PlaceUse {
    pub scope: FileScopeId,
    pub use_id: ScopedUseId,
    pub place: ScopedPlaceId,
}

/// Indexed by root AST node while the flow builder visits expressions in evaluation order.
/// String annotations have sparse node indices outside the root tree.
#[derive(Debug)]
pub(super) struct AstIdsBuilder {
    root: Box<[NodeIds]>,
    annotations: BTreeMap<ast::NodeIndex, NodeIds>,
}

fn root_index(node: ast::NodeIndex) -> Option<usize> {
    node.as_u32()
        .filter(|&index| index <= ast::MAX_REAL_INDEX)
        .map(|index| index as usize)
}

impl AstIdsBuilder {
    pub(super) fn with_node_count(node_count: usize) -> Self {
        Self {
            root: vec![NodeIds::default(); node_count].into_boxed_slice(),
            annotations: BTreeMap::new(),
        }
    }

    fn entry(&mut self, node: ast::NodeIndex) -> &mut NodeIds {
        if let Some(index) = root_index(node) {
            &mut self.root[index]
        } else {
            self.annotations.entry(node).or_default()
        }
    }

    pub(super) fn record_expression(&mut self, expr: &impl HasTrackedScope, scope: FileScopeId) {
        self.entry(expr.node_index().load()).scope = Some(scope);
    }

    /// Records a use on a non-expression node, such as a function name or the `**kwargs`
    /// keyword in a call. These nodes can have a use ID without an explicitly recorded scope.
    pub(super) fn record_other_use(
        &mut self,
        key: impl Into<ExpressionNodeKey>,
        use_id: ScopedUseId,
    ) {
        self.entry(key.into().index()).use_id = Some(use_id);
    }

    pub(super) fn record_place_use(
        &mut self,
        expr: &ast::Expr,
        scope: FileScopeId,
        use_id: ScopedUseId,
    ) {
        *self.entry(ExpressionNodeKey::from(expr).index()) = NodeIds {
            scope: Some(scope),
            use_id: Some(use_id),
        };
    }

    pub(super) fn try_scoped_use(
        &self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> Option<ScopedUseId> {
        let node = key.into().index();
        let row = if let Some(index) = root_index(node) {
            self.root.get(index)?
        } else {
            self.annotations.get(&node)?
        };
        row.use_id.filter(|_| row.scope == Some(scope))
    }

    /// Stores use IDs in node-index order, with string-annotation nodes after the root nodes.
    /// The IDs themselves keep the order in which the flow builder visited uses in each scope.
    pub(super) fn finish(self) -> AstIds {
        let mut pages: Vec<UsePage> = Vec::new();
        let mut uses = Vec::new();
        let mut scopes = Vec::new();
        let root = self.root.into_iter().enumerate().map(|(index, row)| {
            let index = u32::try_from(index).expect("root AST node indices fit in u32");
            (ast::NodeIndex::from(index), row)
        });
        for (node, row) in root.chain(self.annotations) {
            if let Some(scope) = row.scope {
                ScopeRange::record(&mut scopes, node, scope);
            }
            let Some(use_id) = row.use_id else { continue };
            if let Some(page) = pages.last_mut()
                && let Some(bit) = page.bit(node)
            {
                page.mask |= bit;
            } else {
                pages.push(UsePage {
                    first_node: node,
                    first_use: u32::try_from(uses.len()).expect("at most one use per AST node"),
                    mask: 1,
                });
            }
            uses.push(use_id);
        }
        AstIds {
            pages: pages.into_boxed_slice(),
            uses: uses.into_boxed_slice(),
            scopes: scopes.into_boxed_slice(),
        }
    }
}

/// The set bits locate uses within 64 node indices of the first use on this page.
/// Gaps between pages, including the gap before quoted annotations, occupy no storage.
#[derive(Debug, get_size2::GetSize)]
struct UsePage {
    first_node: ast::NodeIndex,
    first_use: u32,
    mask: u64,
}

impl UsePage {
    fn bit(&self, node: ast::NodeIndex) -> Option<u64> {
        if node == self.first_node {
            return Some(1);
        }
        let offset = node.as_u32()?.checked_sub(self.first_node.as_u32()?)?;
        1_u64.checked_shl(offset)
    }
}

#[derive(Debug, get_size2::GetSize)]
struct ScopeRange {
    start: ast::NodeIndex,
    end: ast::NodeIndex,
    scope: FileScopeId,
}

impl ScopeRange {
    /// Unmarked identifiers between two expressions inherit their scope if both agree.
    fn record(ranges: &mut Vec<Self>, node: ast::NodeIndex, scope: FileScopeId) {
        if let Some(last) = ranges.last_mut()
            && last.scope == scope
        {
            last.end = node;
        } else {
            ranges.push(Self {
                start: node,
                end: node,
                scope,
            });
        }
    }
}

#[derive(Debug, get_size2::GetSize)]
pub(crate) struct AstIds {
    pages: Box<[UsePage]>,
    uses: Box<[ScopedUseId]>,
    scopes: Box<[ScopeRange]>,
}

impl AstIds {
    /// Looks up a node in the packed use array. Each earlier set bit on the same page
    /// accounts for one preceding entry in the array.
    fn source_use(&self, node: ast::NodeIndex) -> Option<ScopedUseId> {
        let end = self.pages.partition_point(|page| page.first_node <= node);
        let page = self.pages[..end].last()?;
        let bit = page.bit(node)?;
        if page.mask & bit == 0 {
            return None;
        }
        let preceding = (page.mask & (bit - 1)).count_ones();
        Some(self.uses[(page.first_use + preceding) as usize])
    }

    fn scope_at(&self, node: ast::NodeIndex) -> Option<FileScopeId> {
        let index = self.scopes.partition_point(|range| range.end < node);
        let range = self.scopes.get(index)?;
        (range.start <= node).then_some(range.scope)
    }

    pub(super) fn expression_scope(&self, node: &impl HasTrackedScope) -> Option<FileScopeId> {
        self.scope_at(node.node_index().load())
    }

    pub(super) fn scope_and_use(
        &self,
        expr: ast::ExprRef<'_>,
    ) -> Option<(FileScopeId, ScopedUseId)> {
        let node = ExpressionNodeKey::from(expr).index();
        let use_id = self.source_use(node)?;
        Some((self.scope_at(node)?, use_id))
    }

    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.try_use_id(key)
            .expect("no use ID for this source occurrence")
    }

    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        self.source_use(key.into().index())
    }
}

fn ast_ids<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> &'db AstIds {
    semantic_index(db, file).ast_ids()
}

/// Uniquely identifies a use of a name in a [`crate::FileScopeId`].
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedUseId;

pub trait HasScopedUseId {
    /// Returns the ID that uniquely identifies the use in its scope.
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId;
}

impl HasScopedUseId for ast::Identifier {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
        ast_ids.use_id(self)
    }
}

impl HasScopedUseId for ast::ExprName {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::ExprAttribute {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::ExprSubscript {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let expression_ref = ExprRef::from(self);
        expression_ref.scoped_use_id(db, file)
    }
}

impl HasScopedUseId for ast::Keyword {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
        ast_ids.use_id(self)
    }
}

impl HasScopedUseId for ast::ExprRef<'_> {
    fn scoped_use_id(&self, db: &dyn Db, file: ProgramFile<'_>) -> ScopedUseId {
        let ast_ids = ast_ids(db, file);
        ast_ids.use_id(*self)
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast as ast;

    use crate::{ast_node_ref::AstNodeRef, node_key::NodeKey};

    #[derive(
        Copy,
        Clone,
        Eq,
        PartialEq,
        Ord,
        PartialOrd,
        Hash,
        Debug,
        get_size2::GetSize,
        salsa::SalsaValue,
    )]
    pub struct ExpressionNodeKey(NodeKey);

    impl ExpressionNodeKey {
        pub(super) fn index(self) -> ast::NodeIndex {
            self.0.index()
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_ids_across_page_boundary() {
        let mut builder = AstIdsBuilder::with_node_count(65);
        for (use_id, node) in [64u32, 0, 63].into_iter().enumerate() {
            builder.entry(node.into()).use_id = Some(ScopedUseId::from_usize(use_id));
        }

        let ids = builder.finish();
        let lookup = |node: u32| ids.source_use(node.into()).map(ScopedUseId::as_u32);
        assert_eq!(lookup(0), Some(1));
        assert_eq!(lookup(62), None);
        assert_eq!(lookup(63), Some(2));
        assert_eq!(lookup(64), Some(0));
    }
}
