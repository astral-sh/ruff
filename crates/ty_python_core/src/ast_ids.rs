use std::collections::BTreeMap;

use ruff_index::newtype_index;
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;

use crate::Db;
use crate::HasTrackedScope;
use crate::ProgramFile;
use crate::frozen::FrozenMap;
use crate::place::ScopedPlaceId;
use crate::scope::FileScopeId;
use crate::semantic_index;

pub use node_key::ExpressionNodeKey;

/// Source coordinates for an occurrence in the indexed syntax.
#[derive(Clone, Copy, Default, Debug)]
struct SourceOccurrence {
    scope: Option<FileScopeId>,
    use_id: Option<ScopedUseId>,
}

/// The lexical place and its reaching-definition operand at an expression.
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
    root: Box<[SourceOccurrence]>,
    annotations: BTreeMap<ast::NodeIndex, SourceOccurrence>,
}

fn root_index(node: ast::NodeIndex) -> Option<usize> {
    node.as_u32()
        .filter(|&index| index <= ast::MAX_REAL_INDEX)
        .map(|index| index as usize)
}

impl AstIdsBuilder {
    pub(super) fn with_node_count(node_count: usize) -> Self {
        Self {
            root: vec![SourceOccurrence::default(); node_count].into_boxed_slice(),
            annotations: BTreeMap::new(),
        }
    }

    fn entry(&mut self, node: ast::NodeIndex) -> &mut SourceOccurrence {
        if let Some(index) = root_index(node) {
            &mut self.root[index]
        } else {
            self.annotations.entry(node).or_default()
        }
    }

    pub(super) fn record_expression(&mut self, expr: &impl HasTrackedScope, scope: FileScopeId) {
        self.entry(expr.node_index().load()).scope = Some(scope);
    }

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
        *self.entry(ExpressionNodeKey::from(expr).index()) = SourceOccurrence {
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

    /// Keep one row per use and one range per run of expressions in the same scope.
    /// Walking the root slots in source order also puts out-of-order visits in their final order.
    pub(super) fn finish(self) -> AstIds {
        let mut scopes = Vec::new();
        let mut uses = Vec::new();
        let pages = self
            .root
            .chunks(PAGE_SIZE)
            .enumerate()
            .map(|(page, rows)| {
                let first_use = u32::try_from(uses.len()).expect("at most one use per root node");
                // The last range from the previous page can extend into or across this one.
                let first_scope = u32::try_from(scopes.len().saturating_sub(1))
                    .expect("at most one scope range per root node");
                let mut mask = 0;
                for (offset, row) in rows.iter().enumerate() {
                    if let Some(scope) = row.scope {
                        let index = u32::try_from(page * PAGE_SIZE + offset)
                            .expect("root AST node indices fit in u32");
                        ScopeRange::record(&mut scopes, ast::NodeIndex::from(index), scope);
                    }
                    if let Some(use_id) = row.use_id {
                        mask |= 1 << offset;
                        uses.push(use_id);
                    }
                }
                SourcePage {
                    first_use,
                    first_scope,
                    mask,
                }
            })
            .collect();

        let mut annotation_uses = Vec::new();
        for (node, row) in self.annotations {
            if let Some(scope) = row.scope {
                ScopeRange::record(&mut scopes, node, scope);
            }
            if let Some(use_id) = row.use_id {
                annotation_uses.push((node, use_id));
            }
        }
        AstIds {
            pages,
            uses: uses.into_boxed_slice(),
            scopes: scopes.into_boxed_slice(),
            annotation_uses: FrozenMap::from_entries(annotation_uses),
        }
    }
}

const PAGE_SIZE: usize = u64::BITS as usize;

/// Locates uses and scope ranges for 64 consecutive root AST nodes.
/// Each set bit corresponds to one consecutive entry in `AstIds::uses`.
#[derive(Debug, get_size2::GetSize)]
struct SourcePage {
    first_use: u32,
    first_scope: u32,
    mask: u64,
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
    pages: Box<[SourcePage]>,
    uses: Box<[ScopedUseId]>,
    scopes: Box<[ScopeRange]>,
    annotation_uses: FrozenMap<ast::NodeIndex, ScopedUseId>,
}

impl AstIds {
    fn source_use(&self, node: ast::NodeIndex) -> Option<ScopedUseId> {
        let Some(index) = root_index(node) else {
            return self.annotation_uses.get(&node).copied();
        };
        let page = self.pages.get(index / PAGE_SIZE)?;
        let bit = 1_u64 << (index % PAGE_SIZE);
        if page.mask & bit == 0 {
            return None;
        }
        let preceding = (page.mask & (bit - 1)).count_ones();
        Some(self.uses[(page.first_use + preceding) as usize])
    }

    fn scope_at(&self, node: ast::NodeIndex) -> Option<FileScopeId> {
        // Most pages fall in a single scope. Even with nested scopes, at most one new
        // range can start per node in the page.
        let range = if let Some(page) = root_index(node).and_then(|i| self.pages.get(i / PAGE_SIZE))
        {
            self.scopes[page.first_scope as usize..]
                .iter()
                .find(|range| node <= range.end)?
        } else {
            let index = self.scopes.partition_point(|range| range.end < node);
            self.scopes.get(index)?
        };
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
mod source_occurrence_tests {
    use super::*;

    #[test]
    fn large_operands_preserve_each_index() {
        let mut builder = AstIdsBuilder::with_node_count(65);
        let cases = [
            (0, 0, 0),
            (63, 65_534, 65_535),
            (64, u32::MAX - 1, u32::MAX - 1),
            (ast::MAX_REAL_INDEX + 1, u32::MAX - 1, u32::MAX - 1),
        ];
        for (node, scope, use_id) in cases {
            *builder.entry(ast::NodeIndex::from(node)) = SourceOccurrence {
                scope: Some(FileScopeId::from_u32(scope)),
                use_id: Some(ScopedUseId::from_u32(use_id)),
            };
        }
        let ids = builder.finish();
        for (node, scope, use_id) in cases {
            let node = ast::NodeIndex::from(node);
            let scope = FileScopeId::from_u32(scope);
            assert_eq!(ids.source_use(node), Some(ScopedUseId::from_u32(use_id)));
            assert_eq!(ids.scope_at(node), Some(scope));
        }
        assert_eq!(ids.source_use(ast::NodeIndex::from(62)), None);
    }

    #[test]
    fn scope_gaps_match_source_order_across_blocks_and_string_annotations() {
        let mut builder = AstIdsBuilder::with_node_count(711);
        let annotation = ast::MAX_REAL_INDEX + 1;
        let inputs = [
            (75, 0),
            (5, 2),
            (annotation + 70, 3),
            (13, 0),
            (annotation + 11, 0),
            (701, 0),
            (710, 0),
            (73, 1),
            (annotation + 33, 3),
            (63, 0),
            (1, 2),
        ];
        let mut points = BTreeMap::new();
        for (index, scope) in inputs {
            let index = ast::NodeIndex::from(index);
            let scope = FileScopeId::from_u32(scope);
            builder.entry(index).scope = Some(scope);
            points.insert(index, scope);
        }

        let ids = builder.finish();
        for i in (0..740).chain(annotation - 20..annotation + 74) {
            let node = ast::NodeIndex::from(i);
            let previous = points.range(..=node).next_back().map(|(_, scope)| *scope);
            let next = points.range(node..).next().map(|(_, scope)| *scope);
            assert_eq!(
                ids.scope_at(node),
                previous.filter(|_| previous == next),
                "node {i}"
            );
        }
    }
    #[test]
    fn direct_operands_survive_far_revisits_and_empty_pages() {
        let mut builder = AstIdsBuilder::with_node_count(9501);
        let scope = FileScopeId::from_u32(88);
        let other = FileScopeId::from_u32(78);
        let use_id = ScopedUseId::from_u32(10);
        let row = SourceOccurrence {
            scope: Some(scope),
            use_id: Some(use_id),
        };
        *builder.entry(ast::NodeIndex::from(0)) = row;
        for i in 16..360 {
            *builder.entry(ast::NodeIndex::from(i)) = row;
        }
        let late = ast::NodeIndex::from(3);
        *builder.entry(late) = row;
        builder.entry(late).scope = Some(other);
        builder.entry(ast::NodeIndex::from(9500)).scope = Some(scope);
        let ids = builder.finish();
        assert_eq!(ids.source_use(late), Some(use_id));
        assert_eq!(ids.scope_at(late), Some(other));
        assert_eq!(ids.scope_at(ast::NodeIndex::from(7000)), Some(scope));
        assert_eq!(ids.source_use(ast::NodeIndex::from(7000)), None);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(9999)), None);
    }

    #[test]
    fn unscoped_wide_use_is_transparent_to_gap_lookups() {
        let mut builder = AstIdsBuilder::with_node_count(10_006);
        let scope = FileScopeId::from_u32(65_534);
        let center = ast::NodeIndex::from(134);
        builder.entry(ast::NodeIndex::from(2)).scope = Some(scope);
        builder.entry(ast::NodeIndex::from(10_005)).scope = Some(scope);
        let wide = ScopedUseId::from_u32(u32::MAX - 1);
        builder.entry(center).use_id = Some(wide);
        let ids = builder.finish();
        assert_eq!(ids.scope_at(center), Some(scope));
        assert_eq!(ids.source_use(center), Some(wide));
    }
}
