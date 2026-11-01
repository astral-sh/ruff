use ruff_index::newtype_index;
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::HasTrackedScope;
use crate::frozen::PackedIds;
use crate::member::ScopedMemberId;
use crate::place::ScopedPlaceId;
use crate::scope::FileScopeId;
use crate::symbol::ScopedSymbolId;

pub use node_key::ExpressionNodeKey;

/// The scope and use IDs recorded for one AST node during indexing.
#[derive(Clone, Copy, Default, Debug)]
struct NodeIds {
    scope: Option<FileScopeId>,
    use_id: Option<ScopedUseId>,
    /// Zero for non-expression uses, such as function names and splatted keywords.
    place_plus_one: u32,
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

/// Records lexical uses by root AST row, with separately parsed string annotations in a sparse index space.
#[derive(Debug)]
pub(super) struct AstIdsBuilder {
    root: Box<[NodeIds]>,
    annotations: FxHashMap<ast::NodeIndex, NodeIds>,
}

impl AstIdsBuilder {
    pub(super) fn with_node_count(node_count: usize) -> Self {
        Self {
            root: vec![NodeIds::default(); node_count].into_boxed_slice(),
            annotations: FxHashMap::default(),
        }
    }

    fn entry(&mut self, node: ast::NodeIndex) -> &mut NodeIds {
        if let Some(row) = node
            .as_u32()
            .and_then(|index| self.root.get_mut(index as usize))
        {
            row
        } else {
            self.annotations.entry(node).or_default()
        }
    }

    pub(super) fn record_expression(&mut self, expr: &impl HasTrackedScope, scope: FileScopeId) {
        self.entry(expr.node_index()).scope = Some(scope);
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
        expr: ast::Expr<'_>,
        scope: FileScopeId,
        use_id: ScopedUseId,
        place: ScopedPlaceId,
    ) {
        let ordinal = match place {
            ScopedPlaceId::Symbol(id) => id.as_u32(),
            ScopedPlaceId::Member(id) => id.as_u32(),
        };
        *self.entry(ExpressionNodeKey::from(expr).index()) = NodeIds {
            scope: Some(scope),
            use_id: Some(use_id),
            place_plus_one: ordinal + 1,
        };
    }

    pub(super) fn try_scoped_use(
        &self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> Option<ScopedUseId> {
        let node = key.into().index();
        let row = node
            .as_u32()
            .and_then(|index| self.root.get(index as usize))
            .or_else(|| self.annotations.get(&node))?;
        row.use_id.filter(|_| row.scope == Some(scope))
    }

    /// Stores use IDs and their lexical places in node-index order.
    /// The IDs themselves keep the order in which the flow builder visited uses in each scope.
    pub(super) fn finish(self) -> AstIds {
        let mut pages: Vec<UsePage> = Vec::new();
        let mut use_ids = Vec::new();
        let mut places_plus_one = Vec::new();
        let mut scopes = Vec::new();
        let mut annotations: Vec<_> = self.annotations.into_iter().collect();
        annotations.sort_unstable_by_key(|(node, _)| *node);
        for (node, row) in (0..)
            .map(ast::NodeIndex::from)
            .zip(self.root)
            .chain(annotations)
        {
            if let Some(scope) = row.scope {
                ScopeRange::record(&mut scopes, node, scope);
            }
            let Some(use_id) = row.use_id else { continue };
            if let Some(page) = pages.last_mut()
                && let Some(bit) = page.bit(node)
            {
                page.mask |= bit;
            } else {
                // Each set bit accounts for one distinct node in the use array.
                let first_use = pages
                    .last()
                    .map_or(0, |last| last.first_use + last.mask.count_ones());
                pages.push(UsePage {
                    first_node: node,
                    first_use,
                    mask: 1,
                });
            }
            use_ids.push(use_id);
            places_plus_one.push(row.place_plus_one);
        }
        AstIds {
            pages: pages.into_boxed_slice(),
            use_ids: use_ids.into(),
            places_plus_one: places_plus_one.into(),
            scopes: scopes.into_boxed_slice(),
        }
    }
}

/// The set bits locate uses within 64 node indices of the first use on this page.
/// Gaps between pages occupy no storage.
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
    use_ids: PackedIds<ScopedUseId>,
    /// Symbol or member ordinals in the same order as `use_ids`. The expression determines
    /// which table to use. Keeping these in the AST index lets an edit change a read's place
    /// without invalidating queries that only depend on reaching bindings.
    places_plus_one: PackedIds<u32>,
    scopes: Box<[ScopeRange]>,
}

impl AstIds {
    /// Looks up a node in the packed use array. Each earlier set bit on the same page
    /// accounts for one preceding entry in the array.
    fn use_index(&self, node: ast::NodeIndex) -> Option<usize> {
        let end = self.pages.partition_point(|page| page.first_node <= node);
        let page = self.pages[..end].last()?;
        let bit = page.bit(node)?;
        if page.mask & bit == 0 {
            return None;
        }
        let preceding = (page.mask & (bit - 1)).count_ones();
        Some((page.first_use + preceding) as usize)
    }

    fn scope_at(&self, node: ast::NodeIndex) -> Option<FileScopeId> {
        let index = self.scopes.partition_point(|range| range.end < node);
        let range = self.scopes.get(index)?;
        (range.start <= node).then_some(range.scope)
    }

    pub(super) fn expression_scope(&self, node: &impl HasTrackedScope) -> Option<FileScopeId> {
        self.scope_at(node.node_index())
    }

    pub(super) fn place_use(&self, expr: ast::Expr<'_>) -> Option<PlaceUse> {
        let node = ExpressionNodeKey::from(expr).index();
        let index = self.use_index(node)?;
        let ordinal = self.places_plus_one.get(index).checked_sub(1)?;
        let place = match expr.classify() {
            ast::ExprKind::Name(_) => ScopedPlaceId::Symbol(ScopedSymbolId::from_u32(ordinal)),
            ast::ExprKind::Attribute(_) | ast::ExprKind::Subscript(_) => {
                ScopedPlaceId::Member(ScopedMemberId::from_u32(ordinal))
            }
            _ => return None,
        };
        Some(PlaceUse {
            scope: self.scope_at(node)?,
            use_id: self.use_ids.get(index),
            place,
        })
    }

    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        self.use_index(key.into().index())
            .map(|index| self.use_ids.get(index))
    }
}

/// Uniquely identifies a use of a name in a [`crate::FileScopeId`].
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedUseId;

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast::{self as ast};

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

    impl From<ast::Expr<'_>> for ExpressionNodeKey {
        fn from(value: ast::Expr<'_>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<ast::ExprName<'_>> for ExpressionNodeKey {
        fn from(value: ast::ExprName<'_>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<ast::ExprCall<'_>> for ExpressionNodeKey {
        fn from(value: ast::ExprCall<'_>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<ast::ExprLambda<'_>> for ExpressionNodeKey {
        fn from(value: ast::ExprLambda<'_>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<ast::Identifier<'_>> for ExpressionNodeKey {
        fn from(value: ast::Identifier<'_>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<ast::Keyword<'_>> for ExpressionNodeKey {
        fn from(value: ast::Keyword<'_>) -> Self {
            Self(NodeKey::from_view(value))
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
    use ruff_db::parsed::parsed_string_annotation;
    use ruff_python_ast::AstNode as _;
    use ruff_python_parser::{ParseError, parse_module};

    use super::*;

    #[test]
    fn use_ids_across_page_boundary() {
        let mut builder = AstIdsBuilder::with_node_count(129);
        for (use_id, node) in [64u32, 0, 63, 128].into_iter().enumerate() {
            builder.entry(node.into()).use_id = Some(ScopedUseId::from_usize(use_id));
        }

        let ids = builder.finish();
        let lookup = |node: u32| {
            ids.use_index(node.into())
                .map(|index| ids.use_ids.get(index).as_u32())
        };
        assert_eq!(lookup(0), Some(1));
        assert_eq!(lookup(62), None);
        assert_eq!(lookup(63), Some(2));
        assert_eq!(lookup(64), Some(0));
        assert_eq!(lookup(128), Some(3));
    }
    #[test]
    fn uses_across_blocks_scopes_and_annotations() -> Result<(), ParseError> {
        let source = format!("({}a, b, c, 'T')", "x, ".repeat(61));
        let mut module = parse_module(&source)?;
        module.index_module();
        let [stmt] = module.suite().as_array().unwrap();
        let expr = stmt.as_expr_stmt().unwrap().value();
        let tuple = expr.as_tuple_expr().unwrap();
        let [a, b, c, string] = tuple.elts().slice(61..).as_array().unwrap();
        assert_eq!(a.node_index().as_u32(), Some(63));
        assert_eq!(b.node_index().as_u32(), Some(64));

        let string = string
            .as_string_literal_expr()
            .unwrap()
            .as_single_part_string()
            .unwrap();
        let annotation = parsed_string_annotation(&source, string)?;
        let t = annotation.expr();

        let mut builder = AstIdsBuilder::with_node_count(module.ast().len());
        let outer = FileScopeId::global();
        let inner = FileScopeId::from_u32(1);
        let (c_id, a_id, b_id, t_id) = (0.into(), 1.into(), 0.into(), 1.into());
        let place = ScopedSymbolId::from_u32(0).into();
        builder.record_place_use(c, outer, c_id, place);
        builder.record_place_use(a, outer, a_id, place);
        builder.record_place_use(b, inner, b_id, place);
        builder.record_place_use(t, inner, t_id, place);
        assert_eq!(builder.try_scoped_use(outer, b), None);
        assert_eq!(builder.try_scoped_use(inner, t), Some(t_id));

        let ids = builder.finish();
        assert_eq!(ids.try_use_id(expr), None);
        assert_eq!(ids.try_use_id(a), Some(a_id));
        assert_eq!(ids.try_use_id(b), Some(b_id));
        assert_eq!(ids.try_use_id(c), Some(c_id));
        assert_eq!(ids.try_use_id(t), Some(t_id));
        Ok(())
    }
}
