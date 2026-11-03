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

/// Source coordinates for an occurrence in the indexed syntax.
#[derive(Clone, Copy, Default, Debug, get_size2::GetSize)]
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

#[derive(Clone, Copy, Debug, Default, get_size2::GetSize)]
struct Operand {
    scope: u16,
    use_id: u16,
}

impl Operand {
    const EXTENDED: u16 = u16::MAX;

    fn from_occurrence(row: SourceOccurrence) -> Option<Self> {
        let scope = u16::try_from(row.scope.map_or(0, |scope| scope.as_u32() + 1)).ok()?;
        if scope == Self::EXTENDED {
            return None;
        }
        Some(Self {
            scope,
            use_id: u16::try_from(row.use_id.map_or(0, |use_id| use_id.as_u32() + 1)).ok()?,
        })
    }

    fn occurrence(self) -> SourceOccurrence {
        debug_assert_ne!(self.scope, Self::EXTENDED);
        SourceOccurrence {
            scope: u32::from(self.scope)
                .checked_sub(1)
                .map(FileScopeId::from_u32),
            use_id: u32::from(self.use_id)
                .checked_sub(1)
                .map(ScopedUseId::from_u32),
        }
    }
}

const PAGE_SIZE: usize = 64;

#[derive(Clone, Debug, get_size2::GetSize)]
struct SourcePage {
    rows: [Operand; PAGE_SIZE],
    has_scope: bool,
}

impl Default for SourcePage {
    fn default() -> Self {
        Self {
            rows: [Operand::default(); PAGE_SIZE],
            has_scope: false,
        }
    }
}

#[derive(Default, Debug, get_size2::GetSize)]
pub(crate) struct AstIds {
    pages: Vec<SourcePage>,
    // Used only for unmarked identifiers, which need the surrounding explicit scopes.
    // The directory lets a lookup cross arbitrarily long empty spans in logarithmic time.
    occupied: std::collections::BTreeSet<usize>,
    // Full-width operands and the sparse indices of separately parsed annotations.
    exceptional: BTreeMap<ast::NodeIndex, SourceOccurrence>,
}

impl AstIds {
    /// Allocate the source slots once using the indexed root tree's existing extent.
    /// Sparse annotation indices are kept separately and do not affect this size.
    pub(super) fn with_node_count(node_count: usize) -> Self {
        Self {
            pages: Vec::with_capacity(node_count.div_ceil(PAGE_SIZE)),
            ..Self::default()
        }
    }

    fn regular_index(node: ast::NodeIndex) -> Option<usize> {
        node.as_u32()
            .filter(|&id| id <= ast::MAX_REAL_INDEX)
            .map(|id| id as usize)
    }

    fn slot(&mut self, index: usize, marks_scope: bool) -> &mut Operand {
        let page = index / PAGE_SIZE;
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, SourcePage::default);
        }
        let entries = &mut self.pages[page];
        if marks_scope && !entries.has_scope {
            entries.has_scope = true;
            self.occupied.insert(page);
        }
        &mut entries.rows[index % PAGE_SIZE]
    }

    fn get(&self, node: ast::NodeIndex) -> SourceOccurrence {
        if let Some(index) = Self::regular_index(node) {
            let Some(page) = self.pages.get(index / PAGE_SIZE) else {
                return SourceOccurrence::default();
            };
            let row = page.rows[index % PAGE_SIZE];
            if row.scope != Operand::EXTENDED {
                return row.occurrence();
            }
        }
        self.exceptional.get(&node).copied().unwrap_or_default()
    }

    fn set(&mut self, node: ast::NodeIndex, value: SourceOccurrence) {
        if let Some(index) = Self::regular_index(node) {
            let slot = self.slot(index, value.scope.is_some());
            if slot.scope != Operand::EXTENDED
                && let Some(compact) = Operand::from_occurrence(value)
            {
                *slot = compact;
                return;
            }
            slot.scope = Operand::EXTENDED;
        }
        self.exceptional.insert(node, value);
    }

    fn mark_scope(&mut self, node: ast::NodeIndex, scope: FileScopeId) {
        if let Some(index) = Self::regular_index(node) {
            let slot = self.slot(index, true);
            if slot.scope != Operand::EXTENDED {
                if scope.as_u32() < u32::from(Operand::EXTENDED - 1) {
                    slot.scope = (scope.as_u32() + 1) as u16;
                    return;
                }
                let mut row = slot.occurrence();
                row.scope = Some(scope);
                slot.scope = Operand::EXTENDED;
                self.exceptional.insert(node, row);
                return;
            }
        }
        self.exceptional.entry(node).or_default().scope = Some(scope);
    }

    pub(super) fn record_expression(&mut self, expr: &impl HasTrackedScope, scope: FileScopeId) {
        self.mark_scope(expr.node_index().load(), scope);
    }

    pub(super) fn record_other_use(
        &mut self,
        key: impl Into<ExpressionNodeKey>,
        use_id: ScopedUseId,
    ) {
        let node = key.into().index();
        let mut row = self.get(node);
        row.use_id = Some(use_id);
        self.set(node, row);
    }

    pub(super) fn record_place_use(
        &mut self,
        expr: &ast::Expr,
        scope: FileScopeId,
        use_id: ScopedUseId,
    ) {
        self.set(
            ExpressionNodeKey::from(expr).index(),
            SourceOccurrence {
                scope: Some(scope),
                use_id: Some(use_id),
            },
        );
    }

    fn scope_in_page(&self, page_id: usize, offset: usize, before: bool) -> Option<FileScopeId> {
        let rows = &self.pages.get(page_id)?.rows;
        let lookup = |i: usize| {
            let scope = rows[i].scope;
            if scope == Operand::EXTENDED {
                let node = ast::NodeIndex::from(u32::try_from(page_id * PAGE_SIZE + i).unwrap());
                self.exceptional[&node].scope
            } else {
                u32::from(scope).checked_sub(1).map(FileScopeId::from_u32)
            }
        };
        if before {
            (0..=offset).rev().find_map(lookup)
        } else {
            (offset..PAGE_SIZE).find_map(lookup)
        }
    }

    fn bordering_scope(&self, id: usize, before: bool) -> Option<FileScopeId> {
        let page = id / PAGE_SIZE;
        if let Some(scope) = self.scope_in_page(page, id % PAGE_SIZE, before) {
            return Some(scope);
        }
        if before {
            let previous = self.occupied.range(..page).next_back()?;
            self.scope_in_page(*previous, PAGE_SIZE - 1, true)
        } else {
            let next = self.occupied.range(page + 1..).next()?;
            self.scope_in_page(*next, 0, false)
        }
    }

    fn scope_at(&self, node: ast::NodeIndex) -> Option<FileScopeId> {
        if let Some(scope) = self.get(node).scope {
            return Some(scope);
        }
        let first_annotation = ast::NodeIndex::from(ast::MAX_REAL_INDEX + 1);
        let (prev, next) = if let Some(id) = Self::regular_index(node) {
            (
                self.bordering_scope(id, true)?,
                self.bordering_scope(id, false).or_else(|| {
                    self.exceptional
                        .range(first_annotation..)
                        .find_map(|(_, row)| row.scope)
                })?,
            )
        } else {
            (
                self.exceptional
                    .range(first_annotation..node)
                    .rev()
                    .find_map(|(_, row)| row.scope)
                    .or_else(|| self.bordering_scope(ast::MAX_REAL_INDEX as usize, true))?,
                self.exceptional
                    .range(node..)
                    .find_map(|(_, row)| row.scope)?,
            )
        };
        (prev == next).then_some(prev)
    }
    pub(super) fn expression_scope(&self, node: &impl HasTrackedScope) -> Option<FileScopeId> {
        self.scope_at(node.node_index().load())
    }

    pub(super) fn scope_and_use(
        &self,
        expr: ast::ExprRef<'_>,
    ) -> Option<(FileScopeId, ScopedUseId)> {
        let row = self.get(ExpressionNodeKey::from(expr).index());
        Some((row.scope?, row.use_id?))
    }

    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.try_use_id(key)
            .expect("no use ID for this source occurrence")
    }

    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        self.get(key.into().index()).use_id
    }

    pub(super) fn try_scoped_use(
        &self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> Option<ScopedUseId> {
        let row = self.get(key.into().index());
        (row.scope == Some(scope)).then_some(row.use_id).flatten()
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
        let node = ast::NodeIndex::from(7);
        let mut ids = AstIds::default();
        for (scope, use_id) in [
            (0, 0),
            (65533, 65534),
            (65534, 0),
            (0, 65535),
            (u32::MAX - 1, u32::MAX - 1),
        ] {
            let scope = FileScopeId::from_u32(scope);
            let use_id = ScopedUseId::from_u32(use_id);
            ids.set(
                node,
                SourceOccurrence {
                    scope: Some(scope),
                    use_id: Some(use_id),
                },
            );
            let row = ids.get(node);
            assert_eq!((row.scope, row.use_id), (Some(scope), Some(use_id)));
        }
        assert_eq!(ids.exceptional.len(), 1);
    }

    #[test]
    fn scope_gaps_match_source_order_across_blocks_and_string_annotations() {
        let mut ids = AstIds::default();
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
            ids.mark_scope(index, scope);
            points.insert(index, scope);
        }

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
        let mut ids = AstIds::default();
        let scope = FileScopeId::from_u32(88);
        let other = FileScopeId::from_u32(78);
        let row = SourceOccurrence {
            scope: Some(scope),
            use_id: Some(ScopedUseId::from_u32(10)),
        };
        ids.set(ast::NodeIndex::from(0), row);
        for i in 16..360 {
            ids.set(ast::NodeIndex::from(i), row);
        }
        let late = ast::NodeIndex::from(3);
        ids.set(late, row);
        assert!(!ids.exceptional.contains_key(&late));
        ids.mark_scope(late, other);
        let back = ids.get(late);
        assert_eq!((back.scope, back.use_id), (Some(other), row.use_id));
        let far = ast::NodeIndex::from(9500);
        ids.mark_scope(far, scope);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(7000)), Some(scope));
        ids.mark_scope(far, other);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(7000)), None);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(9999)), None);
        // An ordinary overflow entry must never be mistaken for a string-annotation mark.
        let ann = ast::NodeIndex::from(ast::MAX_REAL_INDEX + 25);
        ids.mark_scope(ann, other);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(9999)), Some(other));
    }

    #[test]
    fn unscoped_wide_use_is_transparent_to_gap_lookups() {
        let mut ids = AstIds::default();
        let scope = FileScopeId::from_u32(65_534);
        let center = ast::NodeIndex::from(134);
        ids.mark_scope(ast::NodeIndex::from(2), scope);
        ids.mark_scope(ast::NodeIndex::from(10_005), scope);
        let wide = ScopedUseId::from_u32(u32::MAX - 1);
        ids.set(
            center,
            SourceOccurrence {
                use_id: Some(wide),
                ..SourceOccurrence::default()
            },
        );
        assert_eq!(ids.scope_at(center), Some(scope));
        ids.mark_scope(center, FileScopeId::global());
        assert_eq!(ids.get(center).use_id, Some(wide));
        assert_eq!(ids.scope_at(ast::NodeIndex::from(100)), None);
        assert_eq!(ids.scope_at(ast::NodeIndex::from(9_000)), None);
    }
}
