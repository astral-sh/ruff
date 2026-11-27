use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::ExprRef;
use rustc_hash::FxHashMap;

use crate::Db;
use crate::HasTrackedScope;
use crate::ProgramFile;
use crate::definition::{Definition, DefinitionNodeKey};
use crate::frozen::{FrozenMap, PackedIds};
use crate::member::ScopedMemberId;
use crate::place::ScopedPlaceId;
use crate::scope::FileScopeId;
use crate::semantic_index;
use crate::symbol::ScopedSymbolId;

pub use node_key::ExpressionNodeKey;

#[newtype_index]
struct NodeDefinitionId;

/// The scope, definition and use IDs recorded for one AST node during indexing.
#[derive(Clone, Copy, Default, Debug)]
struct NodeIds {
    definition: Option<NodeDefinitionId>,
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

/// Indexed by root AST node while the flow builder visits expressions in evaluation order.
/// Inference parses quoted annotations separately and resolves their names from syntax.
#[derive(Debug)]
pub(super) struct AstIdsBuilder<'db> {
    definitions: IndexVec<NodeDefinitionId, Definition<'db>>,
    star_import_definitions: FxHashMap<DefinitionNodeKey, Vec<Definition<'db>>>,
    root: Box<[NodeIds]>,
}

fn root_index(node: ast::NodeIndex) -> usize {
    node.as_u32()
        .expect("expected an indexed node from the root AST") as usize
}

impl<'db> AstIdsBuilder<'db> {
    /// Associates a node with a single definition, returning any previous definition.
    pub(super) fn record_single_definition(
        &mut self,
        key: DefinitionNodeKey,
        definition: Definition<'db>,
    ) -> Option<Definition<'db>> {
        let row = &mut self.root[root_index(key.index())];
        if let Some(id) = row.definition {
            return Some(std::mem::replace(&mut self.definitions[id], definition));
        }
        row.definition = Some(self.definitions.push(definition));
        None
    }

    /// Keeps the alias queryable even if the wildcard import is invalid, cannot be
    /// resolved, or binds no names.
    pub(super) fn register_star_import(&mut self, key: DefinitionNodeKey) {
        self.star_import_definitions.entry(key).or_default();
    }

    /// Records one name bound by a wildcard import. If the alias binds exactly one name,
    /// [`Self::finish`] packs its definition alongside the regular definitions.
    pub(super) fn record_star_import_definition(
        &mut self,
        key: DefinitionNodeKey,
        definition: Definition<'db>,
    ) {
        self.star_import_definitions
            .entry(key)
            .or_default()
            .push(definition);
    }

    pub(super) fn definition(&self, key: DefinitionNodeKey) -> Option<&Definition<'db>> {
        let row = &self.root[root_index(key.index())];
        Some(&self.definitions[row.definition?])
    }

    pub(super) fn with_node_count(node_count: usize) -> Self {
        Self {
            definitions: IndexVec::new(),
            star_import_definitions: FxHashMap::default(),
            root: vec![NodeIds::default(); node_count].into_boxed_slice(),
        }
    }

    fn entry(&mut self, node: ast::NodeIndex) -> &mut NodeIds {
        &mut self.root[root_index(node)]
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
        place: ScopedPlaceId,
    ) {
        let ordinal = match place {
            ScopedPlaceId::Symbol(id) => id.as_u32(),
            ScopedPlaceId::Member(id) => id.as_u32(),
        };
        let row = self.entry(ExpressionNodeKey::from(expr).index());
        row.scope = Some(scope);
        row.use_id = Some(use_id);
        row.place_plus_one = ordinal + 1;
    }

    pub(super) fn try_scoped_use(
        &self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> Option<ScopedUseId> {
        let node = key.into().index();
        let row = &self.root[root_index(node)];
        row.use_id.filter(|_| row.scope == Some(scope))
    }

    /// Packs definitions, use IDs and their lexical places in node-index order.
    /// The IDs themselves keep the order in which the flow builder visited uses in each scope.
    pub(super) fn finish(mut self) -> AstIds<'db> {
        let star_imports = std::mem::take(&mut self.star_import_definitions);
        let mut non_single_star_imports = Vec::with_capacity(star_imports.len());
        #[expect(
            clippy::iter_over_hash_type,
            reason = "each wildcard import is independently partitioned by definition count"
        )]
        for (key, definitions) in star_imports {
            if let [definition] = definitions.as_slice() {
                self.record_single_definition(key, *definition);
            } else {
                non_single_star_imports.push((key, definitions.into_boxed_slice()));
            }
        }

        let mut definitions = Vec::with_capacity(self.definitions.len());
        let mut pages = Vec::with_capacity(self.root.len().div_ceil(NODES_PER_PAGE));
        let mut use_ids = Vec::new();
        let mut places_plus_one = Vec::new();
        let mut scopes = Vec::new();
        for (first_node, rows) in (0_u32..)
            .step_by(NODES_PER_PAGE)
            .zip(self.root.chunks(NODES_PER_PAGE))
        {
            let first_use = u32::try_from(use_ids.len()).expect("more uses than node indices");
            let first_definition =
                u32::try_from(definitions.len()).expect("more definitions than node indices");
            let mut use_mask = 0;
            let mut definition_mask = 0;
            for (offset, row) in (0_u32..).zip(rows) {
                if let Some(id) = row.definition {
                    definition_mask |= 1_u64 << offset;
                    definitions.push(self.definitions[id]);
                }
                if let Some(scope) = row.scope {
                    let node = ast::NodeIndex::from(first_node + offset);
                    ScopeRange::record(&mut scopes, node, scope);
                }
                if let Some(use_id) = row.use_id {
                    use_mask |= 1_u64 << offset;
                    use_ids.push(use_id);
                    places_plus_one.push(row.place_plus_one);
                }
            }
            pages.push(NodePage {
                first_use,
                first_definition,
                scope_count: u32::try_from(scopes.len())
                    .expect("more scope ranges than node indices"),
                use_mask,
                definition_mask,
            });
        }
        AstIds {
            pages: pages.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
            non_single_star_imports: FrozenMap::from_entries(non_single_star_imports),
            use_ids: use_ids.into(),
            places_plus_one: places_plus_one.into(),
            scopes: scopes.into_boxed_slice(),
        }
    }
}

/// Number of nodes in a full [`NodePage`], one per bit in each mask.
const NODES_PER_PAGE: usize = u64::BITS as usize;

/// Indexes definitions, uses and scope ranges for up to [`NODES_PER_PAGE`] root AST nodes.
/// Keeping the masks and offsets as separate fields saves eight bytes of padding on x86-64.
#[derive(Debug, get_size2::GetSize)]
struct NodePage {
    /// Number of uses on earlier pages. This is the page's starting offset in
    /// `use_ids` and `places_plus_one`.
    first_use: u32,
    /// Number of single definitions on earlier pages.
    first_definition: u32,
    /// Number of scope ranges that start on this page or an earlier page.
    scope_count: u32,
    /// Bit `i` is set if the node at offset `i` on this page has a use ID.
    use_mask: u64,
    /// Bit `i` is set if the node at offset `i` has exactly one definition.
    definition_mask: u64,
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

#[derive(Debug, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct AstIds<'db> {
    pages: Box<[NodePage]>,
    definitions: Box<[Definition<'db>]>,
    /// Wildcard imports that bind zero or multiple names.
    non_single_star_imports: FrozenMap<DefinitionNodeKey, Box<[Definition<'db>]>>,
    use_ids: PackedIds<ScopedUseId>,
    /// Symbol or member ordinals in the same order as `use_ids`. The expression determines
    /// which table to use. Keeping these in the AST index lets an edit change a read's place
    /// without invalidating queries that only depend on reaching bindings.
    places_plus_one: PackedIds<u32>,
    scopes: Box<[ScopeRange]>,
}

/// Each earlier set bit on the same page accounts for one preceding array entry.
fn packed_index(node: usize, mask: u64, first: u32) -> Option<usize> {
    let bit = 1_u64 << (node % NODES_PER_PAGE);
    if mask & bit == 0 {
        return None;
    }
    let preceding = (mask & (bit - 1)).count_ones();
    Some((first + preceding) as usize)
}

impl<'db> AstIds<'db> {
    pub(super) fn definition(&self, key: DefinitionNodeKey) -> Option<&Definition<'db>> {
        let node = key.index().as_u32()? as usize;
        let page = self.pages.get(node / NODES_PER_PAGE)?;
        let index = packed_index(node, page.definition_mask, page.first_definition)?;
        Some(&self.definitions[index])
    }

    pub(super) fn definitions(&self, key: DefinitionNodeKey) -> Option<&[Definition<'db>]> {
        self.definition(key)
            .map(std::slice::from_ref)
            .or_else(|| self.non_single_star_imports.get(&key).map(AsRef::as_ref))
    }

    fn use_index(&self, node: ast::NodeIndex) -> Option<usize> {
        let node = node.as_u32()? as usize;
        let page = self.pages.get(node / NODES_PER_PAGE)?;
        packed_index(node, page.use_mask, page.first_use)
    }

    fn scope_at(&self, node: ast::NodeIndex) -> Option<FileScopeId> {
        let page_index = node.as_u32()? as usize / NODES_PER_PAGE;
        let end = self.pages.get(page_index)?.scope_count as usize;
        // A range started on an earlier page may extend across the boundary.
        let start = page_index.checked_sub(1).map_or(0, |previous| {
            self.pages[previous].scope_count.saturating_sub(1) as usize
        });
        let scopes = &self.scopes[start..end];
        let index = scopes.partition_point(|range| range.end < node);
        let range = scopes.get(index)?;
        (range.start <= node).then_some(range.scope)
    }

    pub(super) fn expression_scope(&self, node: &impl HasTrackedScope) -> Option<FileScopeId> {
        self.scope_at(node.node_index().load())
    }

    pub(super) fn place_use(&self, expr: ast::ExprRef<'_>) -> Option<PlaceUse> {
        let node = ExpressionNodeKey::from(expr).index();
        let index = self.use_index(node)?;
        let ordinal = self.places_plus_one.get(index).checked_sub(1)?;
        let place = match expr {
            ast::ExprRef::Name(_) => ScopedPlaceId::Symbol(ScopedSymbolId::from_u32(ordinal)),
            ast::ExprRef::Attribute(_) | ast::ExprRef::Subscript(_) => {
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

    fn use_id(&self, key: impl Into<ExpressionNodeKey>) -> ScopedUseId {
        self.try_use_id(key)
            .expect("the flow builder should have recorded this use")
    }

    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        self.use_index(key.into().index())
            .map(|index| self.use_ids.get(index))
    }
}

fn ast_ids<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> &'db AstIds<'db> {
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
    fn scopes_across_empty_pages() {
        let mut builder = AstIdsBuilder::with_node_count(385);
        let first = FileScopeId::from_usize(1);
        let second = FileScopeId::from_usize(2);
        for node in [69u32, 192] {
            builder.entry(node.into()).scope = Some(first);
        }
        builder.entry(384u32.into()).scope = Some(second);

        let ids = builder.finish();
        assert_eq!(ids.scope_at(0u32.into()), None);
        assert_eq!(ids.scope_at(68u32.into()), None);
        assert_eq!(ids.scope_at(128u32.into()), Some(first));
        assert_eq!(ids.scope_at(192u32.into()), Some(first));
        assert_eq!(ids.scope_at(193u32.into()), None);
        assert_eq!(ids.scope_at(256u32.into()), None);
        assert_eq!(ids.scope_at(384u32.into()), Some(second));
    }

    #[test]
    fn use_ids_across_page_boundary() {
        let mut builder = AstIdsBuilder::with_node_count(193);
        for (use_id, node) in [64u32, 0, 63, 192].into_iter().enumerate() {
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
        assert_eq!(lookup(128), None);
        assert_eq!(lookup(192), Some(3));
        assert_eq!(lookup(193), None);
        assert_eq!(lookup(256), None);
    }
}
