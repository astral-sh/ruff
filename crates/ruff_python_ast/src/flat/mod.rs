//! Python syntax stored in a table of fixed-size rows with four-byte child handles.
//!
//! A handle identifies a row in one AST. A view borrows that AST to resolve the
//! handle's fields. Growing the backing vectors does not invalidate handles.

use std::fmt;
use std::hash::BuildHasher;
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::num::NonZeroU32;

use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::name::Name;
use hashbrown::{HashTable, hash_table::Entry};
use rustc_hash::FxBuildHasher;

use crate::kind::Any;
use crate::{
    DebugText, MAX_REAL_INDEX, NodeIndex, NodeIndexError, NodeKind, Number, sub_ast_level,
    sub_indices,
};

mod children;
mod generated;
mod list;
mod nodes;
mod rewrite;
mod row;
mod scalar;
mod string_like;
pub use string_like::{StringLike, StringLikeKind, StringLikePart, StringLikePartKind};
pub mod find_node;
pub(crate) mod helpers;
pub mod statement_visitor;
pub mod traversal;
pub mod visitor;

pub use children::{Children, Descendants};
pub use generated::*;
pub use list::{
    FStringPartId, FStringPartView, Iter as ListIter, List, ListId, ListItem, MappingPatternItemId,
    MappingPatternItems, MappingPatternItemsId,
};
pub use nodes::{
    AnyParameter, ArgOrKeyword, ArgumentsSourceOrder, BytesValue, FStringValue, LiteralExpression,
    StringValue, TStringValue,
};
pub use rewrite::Rewriter;

/// A borrowed list of syntax nodes from the same tree.
pub type NodeList<'a, T> = List<'a, AstNodeId<T>>;

/// Statements belonging to a single Python suite, in source order.
pub type Suite<'a> = NodeList<'a, crate::kind::Stmt>;
/// Literal text and interpolation fields in an f-string or t-string.
pub type InterpolatedStringElements<'a> = NodeList<'a, crate::kind::InterpolatedStringElement>;
use list::Interval;
use row::Row;
use scalar::Word;

const ABSENT: u32 = u32::MAX;

/// A field occupies at most two little-endian words in extra data.
#[expect(
    clippy::cast_possible_truncation,
    reason = "The first word contains the low 32 bits."
)]
const fn split_words(field: u64) -> [u32; 2] {
    [field as u32, (field >> 32) as u32]
}

/// The nonzero slot of a node of type `T` within its AST.
///
/// The physical slot of a node of type `T` in one AST. Use [`Ast::view`] to borrow the node.
/// Slot zero represents an absent child. This also lets `Option<AstNodeId<T>>` fit in four bytes.
#[repr(transparent)]
pub struct AstNodeId<T> {
    slot: NonZeroU32,
    ty: PhantomData<fn() -> T>,
}

impl<T> Copy for AstNodeId<T> {}

impl<T> Clone for AstNodeId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for AstNodeId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot
    }
}

impl<T> Eq for AstNodeId<T> {}

impl<T> std::hash::Hash for AstNodeId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.row_number().hash(state);
    }
}

#[cfg(feature = "get-size")]
impl<T> get_size2::GetSize for AstNodeId<T> {}

impl<T> fmt::Debug for AstNodeId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.row_number().fmt(f)
    }
}

impl<T> AstNodeId<T> {
    const fn from_word(word: u32) -> Self {
        Self {
            slot: NonZeroU32::new(word).expect("slot zero is reserved for absent children"),
            ty: PhantomData,
        }
    }

    const fn row_number(self) -> u32 {
        self.slot.get() - 1
    }

    const fn slot(self) -> usize {
        self.slot.get() as usize
    }

    /// Zero-based allocation index for dense per-node tables of size [`Ast::len`].
    pub const fn index(self) -> usize {
        self.row_number() as usize
    }

    /// The key of this row when its arena is a cached module.
    /// Use the view's node index for a quoted annotation, which has its own namespace.
    pub fn module_index(self) -> NodeIndex {
        NodeIndex::from(self.row_number())
    }

    const fn cast<U>(self) -> AstNodeId<U> {
        AstNodeId {
            slot: self.slot,
            ty: PhantomData,
        }
    }
}

const _: () = {
    assert!(size_of::<AstNodeId<crate::kind::Expr>>() == 4);
    assert!(size_of::<Option<AstNodeId<crate::kind::Expr>>>() == 4);
};

/// A child slot used while constructing a tree.
///
/// The builder reserves slot zero before emitting any nodes. Keeping the slot as a plain
/// `u32` avoids validating it on every parser reduction.
#[repr(transparent)]
pub struct BuildId<T> {
    slot: u32,
    ty: PhantomData<fn() -> T>,
}

impl<T> Copy for BuildId<T> {}

impl<T> Clone for BuildId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> fmt::Debug for BuildId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.slot.fmt(f)
    }
}

impl<T> BuildId<T> {
    const fn new(slot: u32) -> Self {
        Self {
            slot,
            ty: PhantomData,
        }
    }

    const fn slot(self) -> usize {
        self.slot as usize
    }

    const fn index(self) -> usize {
        (self.slot - 1) as usize
    }

    const fn cast<U>(self) -> BuildId<U> {
        BuildId::new(self.slot)
    }
}

/// A syntax node and the arena that owns its row.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SyntaxNode<'ast> {
    row: &'ast Row,
    ast: &'ast Ast,
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for SyntaxNode<'_> {}

pub(crate) mod sealed {
    use super::SyntaxNode;

    // Only a checked cast or an AST field with a known type can construct a wrapper.
    pub struct Checked(());
    pub(super) const CHECKED: Checked = Checked(());

    pub trait FromSyntax<'ast> {
        fn from_syntax(syntax: SyntaxNode<'ast>, checked: Checked) -> Self;
    }
}

/// A borrowed Python syntax node. Its marker type can be used in an [`AstNodeId`].
pub trait AstNode<'ast>: sealed::FromSyntax<'ast> + Copy + fmt::Debug + PartialEq + Ranged {
    type Kind: NodeType<Borrowed<'ast> = Self>;

    /// Returns the same row and arena without requiring a particular node kind.
    fn syntax_node(self) -> SyntaxNode<'ast>;

    fn ast(self) -> &'ast Ast {
        self.syntax_node().ast
    }

    fn node_kind(self) -> NodeKind {
        Self::Kind::KIND.unwrap_or_else(|| self.syntax_node().row.kind())
    }

    fn cast<N: AstNode<'ast>>(self) -> Option<N> {
        N::Kind::matches(self.node_kind()).then(|| self.syntax_node().retype())
    }

    /// Recovers the slot for use in another field of the same builder.
    fn build_id(self) -> BuildId<Self::Kind> {
        BuildId::new(self.syntax_node().slot())
    }

    fn node_id(self) -> AstNodeId<Self::Kind> {
        AstNodeId::from_word(self.syntax_node().slot())
    }

    fn is_same_node<'other>(self, other: impl AstNode<'other>) -> bool {
        std::ptr::eq(self.syntax_node().row, other.syntax_node().row)
    }

    fn node_index(self) -> NodeIndex {
        self.syntax_node().node_index()
    }

    /// Yields this node's immediate children in source order.
    #[inline]
    fn children(self) -> Children<'ast> {
        self.syntax_node().children()
    }

    /// Visits this node and its descendants in source order, parents first.
    fn descendants(self) -> Descendants<'ast> {
        self.syntax_node().descendants()
    }

    /// Yields statements in source order, including this node if it is a statement.
    /// Only fields that can contain statements are visited.
    fn descendant_statements(self) -> impl FusedIterator<Item = crate::Stmt<'ast>> {
        children::DescendantStatements::new(self.syntax_node())
    }
}

impl<'ast> SyntaxNode<'ast> {
    #[inline]
    fn from_slot(ast: &'ast Ast, slot: usize) -> Self {
        debug_assert_ne!(slot, 0);
        Self {
            ast,
            row: &ast.rows[slot],
        }
    }

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Builder::push checks that every row index fits in a u32."
    )]
    fn slot(self) -> u32 {
        // SAFETY: The row belongs to this AST. Its distance from the beginning of the
        // allocation is the slot used by IDs and by child fields.
        #[expect(unsafe_code)]
        let slot =
            unsafe { std::ptr::from_ref(self.row).offset_from_unsigned(self.ast.rows.as_ptr()) };
        slot as u32
    }

    fn retype<N: AstNode<'ast>>(self) -> N {
        N::from_syntax(self, sealed::CHECKED)
    }

    /// Yields this node's immediate children in source order.
    #[inline]
    pub fn children(self) -> Children<'ast> {
        self.ast.direct_children(self)
    }

    /// Returns the first immediate child in source order.
    #[inline]
    pub fn first_child(self) -> Option<Self> {
        self.children().next()
    }

    /// Returns the last immediate child by walking the children in source order.
    #[inline]
    pub fn last_child(self) -> Option<Self> {
        self.children().last()
    }

    /// Visits this node and its descendants in source order, parents first.
    pub fn descendants(self) -> Descendants<'ast> {
        Descendants::new(self)
    }

    pub fn node_index(self) -> NodeIndex {
        if matches!(
            self.row.kind(),
            NodeKind::ModModule | NodeKind::ModExpression
        ) {
            return NodeIndex::NONE;
        }
        self.ast.keys.key(self.node_id().row_number())
    }
}

impl<'ast> sealed::FromSyntax<'ast> for SyntaxNode<'ast> {
    fn from_syntax(syntax: Self, _: sealed::Checked) -> Self {
        syntax
    }
}

impl<'ast> AstNode<'ast> for SyntaxNode<'ast> {
    type Kind = crate::kind::Any;

    fn syntax_node(self) -> Self {
        self
    }
}

impl crate::HasNodeIndex for SyntaxNode<'_> {
    fn node_index(&self) -> NodeIndex {
        (*self).node_index()
    }
}

impl Ranged for SyntaxNode<'_> {
    fn range(&self) -> TextRange {
        self.row.range()
    }
}

impl PartialEq for SyntaxNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.is_same_node(*other)
            || (self.node_kind() == other.node_kind()
                && self.range() == other.range()
                && self.node_index() == other.node_index()
                && generated::same_fields(*self, *other))
    }
}

impl fmt::Debug for SyntaxNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            self.fmt_fields(f)
        } else {
            write!(f, "{:?}@{:?}", self.node_kind(), self.range())
        }
    }
}

// Wrappers share their representation and trait implementations. Schema-specific accessors
// and debug implementations live alongside the generated field definitions.
macro_rules! impl_ast_node {
    ($name:ident) => {
        impl<'ast> $name<'ast> {
            /// Yields this node's immediate children in source order.
            #[inline]
            pub fn children(self) -> crate::Children<'ast> {
                crate::AstNode::children(self)
            }
        }

        impl<'ast> crate::flat::sealed::FromSyntax<'ast> for $name<'ast> {
            #[inline]
            fn from_syntax(
                syntax: crate::SyntaxNode<'ast>,
                _: crate::flat::sealed::Checked,
            ) -> Self {
                Self { syntax }
            }
        }

        impl<'ast> crate::AstNode<'ast> for $name<'ast> {
            type Kind = crate::kind::$name;
            #[inline]
            fn syntax_node(self) -> crate::SyntaxNode<'ast> {
                self.syntax
            }
        }

        impl ruff_text_size::Ranged for $name<'_> {
            #[inline]
            fn range(&self) -> ruff_text_size::TextRange {
                self.syntax.range()
            }
        }

        impl crate::HasNodeIndex for $name<'_> {
            #[inline]
            fn node_index(&self) -> crate::NodeIndex {
                self.syntax.node_index()
            }
        }

        #[cfg(feature = "get-size")]
        impl get_size2::GetSize for $name<'_> {}
    };
}
pub(crate) use impl_ast_node;

/// A lifetime-free schema marker that connects a stored node ID to its borrowed node.
pub trait NodeType: Sized + 'static {
    /// The type returned by [`Ast::view`] while the arena is borrowed.
    type Borrowed<'ast>: AstNode<'ast, Kind = Self>;
    const KIND: Option<NodeKind> = None;
    const ANY: bool = false;

    fn matches(kind: NodeKind) -> bool;

    #[doc(hidden)]
    fn replacement<R: Rewriter + ?Sized>(
        _node: Self::Borrowed<'_>,
        _builder: &mut Builder,
        _rewriter: &mut R,
    ) -> Option<BuildId<Self>> {
        None
    }
}

impl SyntaxNode<'_> {
    pub fn is_expression(self) -> bool {
        crate::kind::Expr::matches(self.node_kind())
    }
    pub fn is_statement(self) -> bool {
        crate::kind::Stmt::matches(self.node_kind())
    }
    pub fn is_module(self) -> bool {
        crate::kind::Mod::matches(self.node_kind())
    }
    pub fn kind(self) -> NodeKind {
        self.node_kind()
    }
    pub fn ptr_eq(self, other: Self) -> bool {
        self.is_same_node(other)
    }
}

impl NodeType for crate::kind::Any {
    type Borrowed<'ast> = SyntaxNode<'ast>;
    const ANY: bool = true;

    fn replacement<R: Rewriter + ?Sized>(
        node: SyntaxNode<'_>,
        builder: &mut Builder,
        rewriter: &mut R,
    ) -> Option<BuildId<Self>> {
        if let Some(expr) = node.cast::<crate::Expr>() {
            rewriter.replace_expr(builder, expr).map(BuildId::cast)
        } else if let Some(stmt) = node.cast::<crate::Stmt>() {
            rewriter.replace_stmt(builder, stmt).map(BuildId::cast)
        } else if let Some(literal) = node.cast::<crate::StringLiteral>() {
            rewriter
                .replace_string_literal(builder, literal)
                .map(BuildId::cast)
        } else if let Some(element) = node.cast::<crate::InterpolatedStringElement>() {
            rewriter
                .replace_interpolated_string_element(builder, element)
                .map(BuildId::cast)
        } else {
            None
        }
    }
    fn matches(_: NodeKind) -> bool {
        true
    }
}

/// Translates a zero-based allocation index to a module or string-annotation key.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
enum RowKeys {
    #[default]
    Unindexed,
    Direct(u32),
    /// Only needed when auxiliary rows would exhaust a quoted annotation's reserved keys.
    Annotation {
        first: u32,
        rows: Box<[u32]>,
    },
}

impl RowKeys {
    fn key(&self, row: u32) -> NodeIndex {
        match self {
            Self::Unindexed => NodeIndex::NONE,
            Self::Direct(first) => NodeIndex::from(first + row),
            Self::Annotation { first, rows } => {
                rows.binary_search(&row).map_or(NodeIndex::NONE, |slot| {
                    NodeIndex::from(first + offset(slot))
                })
            }
        }
    }
}

/// Each syntax node occupies one row. Variable-length fields live in the side tables.
/// Variable-length fields and fields that do not fit inline refer to the extra-data arena.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct Ast {
    rows: Vec<Row>,
    extra: Vec<u32>,
    text: String,
    names: Vec<Name>,
    bytes: Vec<u8>,
    numbers: Vec<Number>,
    debug_text: Vec<DebugText>,
    keys: RowKeys,
    /// One past the last yield expression's row, or zero if the tree has none.
    yield_rows_end: u32,
}

impl<'a> crate::StmtFunctionDef<'a> {
    /// Tests the arena rows emitted while parsing the body, including nested scopes.
    /// A positive result still requires a scope-aware walk.
    pub fn body_may_contain_yield(self) -> bool {
        let yield_end = self.syntax.ast.yield_rows_end as usize;
        if yield_end == 0 {
            return false;
        }
        let Some(body) = self.body_row_range() else {
            return true;
        };
        if yield_end <= body.start {
            return false;
        }
        if yield_end <= body.end {
            return true;
        }
        self.syntax.ast.rows[body]
            .iter()
            .any(|row| matches!(row.kind(), NodeKind::ExprYield | NodeKind::ExprYieldFrom))
    }

    /// Includes nested scopes and any rows discarded by parser recovery.
    pub(super) fn body_row_kinds(self) -> Option<impl Iterator<Item = NodeKind> + 'a> {
        Some(
            self.syntax.ast.rows[self.body_row_range()?]
                .iter()
                .map(Row::kind),
        )
    }

    fn body_row_range(self) -> Option<std::ops::Range<usize>> {
        let count = usize::from(self.syntax.row.stmt_function_def_body_row_count());
        if count == 0 {
            return None;
        }
        let end = self.node_id().slot();
        Some(end - count..end)
    }
}

impl Ast {
    /// Whether an identifier with this normalized spelling occurs in the tree.
    /// This scans the distinct names interned during parsing.
    pub fn contains_identifier(&self, spelling: &str) -> bool {
        self.names.iter().any(|name| name.as_str() == spelling)
    }

    #[inline]
    fn extra_word<const WIDE: bool>(&self, slot: usize) -> u64 {
        let low = u64::from(self.extra[slot]);
        if WIDE {
            low | (u64::from(self.extra[slot + 1]) << 32)
        } else {
            low
        }
    }

    pub fn view<T: NodeType>(&self, handle: AstNodeId<T>) -> Option<T::Borrowed<'_>> {
        let row = self.rows.get(handle.slot())?;
        if !T::ANY && !T::matches(row.kind()) {
            return None;
        }
        Some(SyntaxNode { ast: self, row }.retype())
    }

    /// Borrows an untyped row by its allocation index.
    pub fn row(&self, index: u32) -> Option<SyntaxNode<'_>> {
        Some(SyntaxNode {
            ast: self,
            row: self.rows.get((index as usize).checked_add(1)?)?,
        })
    }

    /// Number of stored nodes, excluding the reserved slot.
    pub fn len(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Searches the stored nodes, including any that parser recovery leaves outside the tree.
    pub fn any_node<'ast>(&'ast self, predicate: impl FnMut(SyntaxNode<'ast>) -> bool) -> bool {
        self.rows
            .iter()
            .skip(1)
            .map(|row| SyntaxNode { ast: self, row })
            .any(predicate)
    }

    fn at<T: NodeType>(&self, handle: AstNodeId<T>) -> T::Borrowed<'_> {
        SyntaxNode::from_slot(self, handle.slot()).retype()
    }

    fn child<'ast, N: AstNode<'ast>>(&'ast self, field: u64) -> N {
        SyntaxNode::from_slot(self, split_words(field)[0] as usize).retype()
    }

    fn optional_child<'ast, N: AstNode<'ast>>(&'ast self, field: u64) -> Option<N> {
        (split_words(field)[0] != 0).then(|| self.child(field))
    }

    fn bytes(&self, field: u64) -> &[u8] {
        &self.bytes[Interval::decode(field).indices()]
    }

    fn string(&self, field: u64) -> &str {
        &self.text[Interval::decode(field).indices()]
    }
}

/// Mutable construction state. Nested lists share scratch space, so building the
/// children of a list never interleaves their extra data with its own elements.
#[derive(Debug)]
pub struct Builder {
    ast: Ast,
    source: TextRange,
    scratch: Vec<u32>,
    name_ids: HashTable<u32>,
    recent_names: [u32; 32],
}

/// The index of an identifier spelling in one builder's name pool.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NameId(u32);

impl NameId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The lengths of the rows and side tables before a speculative parse.
pub struct Checkpoint {
    rows: usize,
    extra: usize,
    text: usize,
    names: usize,
    bytes: usize,
    numbers: usize,
    debug_text: usize,
    yield_rows_end: u32,
}

impl Default for Builder {
    fn default() -> Self {
        Self::for_source(TextRange::default())
    }
}

impl Builder {
    /// Start with a small sample of rows so a file with little syntax needs little storage.
    pub fn for_source(source: TextRange) -> Self {
        let bytes = source.len().to_usize();
        let estimated = (bytes / 18).min(4096);
        let estimated_rows = if bytes <= 4096 {
            estimated
        } else {
            estimated.min(128)
        };
        let mut rows = Vec::with_capacity(estimated_rows + 1);
        // No view can refer to this row. Its presence makes every child slot a nonzero index.
        rows.push(Row::new(NodeKind::ModModule, TextRange::default()));
        Self {
            source,
            ast: Ast {
                rows,
                extra: Vec::with_capacity(estimated + estimated / 8),
                names: Vec::with_capacity(estimated / 8),
                ..Ast::default()
            },
            name_ids: HashTable::with_capacity(estimated / 8),
            scratch: Vec::new(),
            recent_names: [0; 32],
        }
    }

    /// Turns a name expression into the identifier for a keyword argument. When the name is
    /// the last row, its expression handle expires and the identifier can reuse that row.
    pub fn take_name(&mut self, name: BuildId<crate::kind::ExprName>) -> build::Identifier {
        let expr = self.get(name);
        let identifier = build::Identifier {
            id: expr.name_id(),
            range: expr.range(),
        };
        if name.slot() + 1 == self.ast.rows.len() {
            self.ast.rows.pop();
        }
        identifier
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            rows: self.ast.rows.len(),
            extra: self.ast.extra.len(),
            text: self.ast.text.len(),
            names: self.ast.names.len(),
            bytes: self.ast.bytes.len(),
            numbers: self.ast.numbers.len(),
            debug_text: self.ast.debug_text.len(),
            yield_rows_end: self.ast.yield_rows_end,
        }
    }

    /// Discards allocations since the checkpoint. Handles to those allocations expire here.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Rewinding consumes the checkpoint so callers cannot reuse it after rebuilding the tree."
    )]
    pub fn rewind(&mut self, checkpoint: Checkpoint) {
        self.ast.rows.truncate(checkpoint.rows);
        self.ast.yield_rows_end = checkpoint.yield_rows_end;
        self.ast.extra.truncate(checkpoint.extra);
        self.ast.text.truncate(checkpoint.text);
        for (i, name) in self.ast.names.drain(checkpoint.names..).enumerate() {
            let index = offset(checkpoint.names + i);
            if let Ok(entry) = self
                .name_ids
                .find_entry(FxBuildHasher.hash_one(name.as_str()), |&id| id == index)
            {
                entry.remove();
            }
        }
        self.ast.bytes.truncate(checkpoint.bytes);
        self.ast.numbers.truncate(checkpoint.numbers);
        self.ast.debug_text.truncate(checkpoint.debug_text);
    }

    pub fn node_count(&self) -> usize {
        self.ast.rows.len() - 1
    }

    /// Records the interval of rows emitted between entering the function body and emitting its
    /// definition. The two remaining payload bytes can describe up to 65,535 rows.
    pub fn record_function_body(
        &mut self,
        function: BuildId<crate::kind::StmtFunctionDef>,
        start: usize,
    ) {
        let end = function.index();
        let count = u16::try_from(end.saturating_sub(start)).unwrap_or(0);
        self.ast.rows[function.slot()].set_stmt_function_def_body_row_count(count);
    }

    pub fn finish_tree<T>(self, root: BuildId<T>) -> Tree<T> {
        Tree {
            ast: Box::new(self.ast),
            root: AstNodeId::from_word(root.slot),
        }
    }

    pub fn get<T: NodeType>(&self, id: BuildId<T>) -> T::Borrowed<'_> {
        let syntax = SyntaxNode::from_slot(&self.ast, id.slot());
        debug_assert!(T::matches(syntax.row.kind()));
        syntax.retype()
    }

    /// Adds a node whose child IDs already belong to this builder.
    pub fn add<N: Emit>(&mut self, node: N) -> BuildId<N::Output> {
        node.emit(self).0
    }

    /// Copies a subtree into this builder so it can be used in new syntax.
    /// The resulting IDs and literal data belong to the destination tree.
    pub fn copy_node<'ast, N: AstNode<'ast>>(&mut self, node: N) -> BuildId<N::Kind> {
        self.copy_with(node, &mut rewrite::Identity)
    }

    /// Copies the given subtrees into a new list, preserving their order.
    pub fn copy_nodes<'a, N: AstNode<'a>>(
        &mut self,
        nodes: impl IntoIterator<Item = N>,
    ) -> ListId<AstNodeId<N::Kind>> {
        self.map_list(nodes, Self::copy_node)
    }

    /// Builds each element in this arena, then stores the resulting elements as a single list.
    pub fn map_list<I, L: ListItem>(
        &mut self,
        items: impl IntoIterator<Item = I>,
        mut map: impl FnMut(&mut Self, I) -> L,
    ) -> ListId<L::Element> {
        self.collect::<L::Element, _>(items, |builder, item| map(builder, item).word())
    }

    #[inline]
    pub fn list<I: ListItem>(&mut self, items: impl IntoIterator<Item = I>) -> ListId<I::Element> {
        let start = self.ast.extra.len();
        self.ast.extra.extend(items.into_iter().map(ListItem::word));
        let len = self.ast.extra.len() - start;
        ListId::new(Interval::new(start, len))
    }

    pub fn get_list<E: list::Element>(&self, list: ListId<E>) -> List<'_, E> {
        List::new(&self.ast, list.word())
    }

    /// Sets the load, store or delete context of an assignment target and its elements.
    pub fn set_expr_context(
        &mut self,
        expr: BuildId<crate::kind::Expr>,
        context: crate::ExprContext,
    ) {
        let slot = expr.slot();
        let encoded = context.encode().to_le_bytes()[0];
        let elements = match self.get(expr).classify() {
            ExprKind::Name(_) => return self.ast.rows[slot].set_expr_name_ctx(encoded),
            ExprKind::Attribute(_) => return self.ast.rows[slot].set_expr_attribute_ctx(encoded),
            ExprKind::Subscript(_) => return self.ast.rows[slot].set_expr_subscript_ctx(encoded),
            ExprKind::Starred(value) => {
                let child = value.value().build_id();
                self.ast.rows[slot].set_expr_starred_ctx(encoded);
                return self.set_expr_context(child, context);
            }
            ExprKind::UnaryOp(value) => {
                return self.set_expr_context(value.operand().build_id(), context);
            }
            ExprKind::Tuple(value) => {
                let elements = value.elts().id();
                self.ast.rows[slot].set_expr_tuple_ctx(encoded);
                elements
            }
            ExprKind::List(value) => {
                let elements = value.elts().id();
                self.ast.rows[slot].set_expr_list_ctx(encoded);
                elements
            }
            _ => return,
        };
        for i in 0..elements.len() {
            if let Some(child) = self.get_list(elements).get(i) {
                self.set_expr_context(child.build_id(), context);
            }
        }
    }

    #[inline]
    fn push<T: NodeType>(&mut self, row: Row) -> BuildId<T> {
        let index = offset(self.ast.rows.len());
        if self.ast.rows.len() < self.ast.rows.capacity() {
            self.ast.rows.push(row);
        } else {
            self.grow_and_push(row);
        }
        BuildId::new(index)
    }

    #[cold]
    fn grow_and_push(&mut self, row: Row) {
        if !self.source.is_empty() {
            self.reserve_rows(row.end());
        }
        self.ast.rows.push(row);
    }

    fn reserve_rows(&mut self, end: TextSize) {
        // A module can start with a long docstring or a license header. Allow its remaining
        // source to be denser than the prefix, and leave some room for variation after it.
        let rows = self.node_count();
        let consumed = end.saturating_sub(self.source.start()).to_usize();
        let remaining = self.source.end().saturating_sub(end).to_usize();
        let estimate = rows.saturating_add(
            remaining
                .saturating_mul(rows)
                .checked_div(consumed)
                .unwrap_or(0)
                .max(remaining / 10),
        );
        let additional = estimate
            .saturating_add(estimate / 8)
            .saturating_sub(rows)
            .clamp(rows.div_ceil(4).max(4), rows.saturating_mul(128).max(4));
        self.ast.rows.reserve_exact(additional);
        let extra = self
            .ast
            .extra
            .len()
            .saturating_mul(additional)
            .checked_div(rows)
            .unwrap_or(0);
        self.ast.extra.reserve_exact(extra);
    }

    fn store_bytes(&mut self, bytes: &[u8]) -> u64 {
        let range = Interval::new(self.ast.bytes.len(), bytes.len());
        self.ast.bytes.extend_from_slice(bytes);
        range.encode()
    }

    /// A string expression stores its decoded text range beside its parts list.
    /// Six bytes hold a text offset and a 16-bit length. Longer strings use a full range
    /// in the extra table, with the maximum inline length reserved for that case.
    fn string_literal_text(&mut self, parts: u64) -> (u32, u16) {
        let [start, len] = split_words(self.join_string_literals(parts).encode());
        if let Ok(len) = u16::try_from(len)
            && len < u16::MAX
        {
            return (start, len);
        }
        let index = offset(self.ast.extra.len());
        self.ast.extra.extend([start, len]);
        (index, u16::MAX)
    }

    fn join_string_literals(&mut self, parts: u64) -> Interval {
        let parts = List::<AstNodeId<crate::kind::StringLiteral>>::new(&self.ast, parts);
        let Some(first) = parts.first() else {
            return Interval::new(0, 0);
        };
        let first = first.text_range();
        if parts.len() == 1 {
            return first;
        }
        let first = first.indices();
        let mut end = first.end;
        if parts.iter().skip(1).all(|part| {
            let range = part.text_range().indices();
            let adjacent = end == range.start;
            end = range.end;
            adjacent
        }) {
            Interval::new(first.start, end - first.start)
        } else {
            // The parser emits adjacent literals consecutively. Explicitly constructed
            // trees may join parts from disjoint positions in the text column.
            let parts = parts.id();
            let start = self.ast.text.len();
            for i in 0..parts.len() {
                let range = self.get_list(parts).at(i).text_range();
                self.ast.text.extend_from_within(range.indices());
            }
            Interval::new(start, self.ast.text.len() - start)
        }
    }

    pub fn intern_name(&mut self, text: &str) -> NameId {
        let names = &mut self.ast.names;
        let bytes = text.as_bytes();
        let first = usize::from(bytes.first().copied().unwrap_or(0));
        let last = usize::from(bytes.last().copied().unwrap_or(0));
        let slot = (first ^ last.wrapping_mul(7) ^ bytes.len().wrapping_mul(13)) & 31;
        let recent = &mut self.recent_names[slot];
        // A speculative parse may have rewound the pool since this entry was cached.
        if names
            .get(*recent as usize)
            .is_some_and(|name| name.as_str() == text)
        {
            return NameId(*recent);
        }
        let index = match self.name_ids.entry(
            FxBuildHasher.hash_one(text),
            |&index| names[index as usize].as_str() == text,
            |&index| FxBuildHasher.hash_one(names[index as usize].as_str()),
        ) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let index = offset(names.len());
                names.push(Name::new(text));
                entry.insert(index);
                index
            }
        };
        *recent = index;
        NameId(index)
    }

    pub fn name(&self, id: NameId) -> &Name {
        &self.ast.names[id.0 as usize]
    }

    fn store_string(&mut self, text: &str) -> u64 {
        let range = Interval::new(self.ast.text.len(), text.len());
        self.ast.text.push_str(text);
        range.encode()
    }

    fn collect<E: list::Element, I>(
        &mut self,
        elements: I,
        mut encode: impl FnMut(&mut Self, I::Item) -> u32,
    ) -> ListId<E>
    where
        I: IntoIterator,
    {
        let start = self.scratch.len();
        for item in elements {
            let word = encode(self, item);
            self.scratch.push(word);
        }
        let items = &self.scratch[start..];
        let range = Interval::new(self.ast.extra.len(), items.len());
        self.ast.extra.extend_from_slice(items);
        self.scratch.truncate(start);
        ListId::new(range)
    }
}

/// An immutable module or expression with a typed root handle.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize), get_size(ignore(T)))]
pub struct Tree<T> {
    ast: Box<Ast>,
    root: AstNodeId<T>,
}

impl<T> Tree<T> {
    pub fn root(&self) -> T::Borrowed<'_>
    where
        T: NodeType,
    {
        self.ast.at(self.root)
    }

    pub fn ast(&self) -> &Ast {
        &self.ast
    }

    pub fn cast<U: NodeType>(self) -> Option<Tree<U>> {
        U::matches(self.ast.rows[self.root.slot()].kind()).then(|| Tree {
            root: self.root.cast(),
            ast: self.ast,
        })
    }

    /// Releases spare capacity in the row and side-table buffers.
    pub fn shrink_to_fit(&mut self) {
        self.ast.rows.shrink_to_fit();
        self.ast.extra.shrink_to_fit();
        self.ast.text.shrink_to_fit();
        self.ast.names.shrink_to_fit();
        self.ast.bytes.shrink_to_fit();
        self.ast.numbers.shrink_to_fit();
        self.ast.debug_text.shrink_to_fit();
    }

    fn required_node_indices(&self) -> usize {
        let count = self.ast.rows.len() - 1;
        // The module or expression wrapper has no NodeIndex. Its row can be omitted
        // from the index range when it comes after every indexed row.
        count - usize::from(self.root.index() + 1 == count)
    }
}

impl Tree<crate::kind::ModModule> {
    /// Uses each node's zero-based allocation index as its module-wide [`NodeIndex`].
    pub fn index_module(&mut self) {
        assert!(
            self.required_node_indices() <= MAX_REAL_INDEX as usize + 1,
            "module exceeds the supported number of rows"
        );
        self.ast.keys = RowKeys::Direct(0);
    }
}

impl Tree<crate::kind::ModExpression> {
    /// Uses the node indices reserved for the annotation's containing string literal.
    pub fn index_string_annotation(&mut self, parent: NodeIndex) -> Result<(), NodeIndexError> {
        let parent = parent.as_u32().ok_or(NodeIndexError::NoParent)?;
        let (first, last) = sub_indices(parent)?;
        let capacity = (last - first + 1) as usize;
        if self.required_node_indices() <= capacity {
            self.ast.keys = RowKeys::Direct(first);
            return Ok(());
        }

        // An f-string's format spec has an arena row, but source-order traversal visits
        // its contents directly. Only reachable syntax needs an index in the annotation.
        let mut rows: Vec<u32> = self
            .root()
            .descendants()
            .skip(1)
            .take(capacity + 1)
            .map(|node| node.node_id().row_number())
            .collect();
        if rows.len() > capacity {
            return Err(if sub_ast_level(parent) == 0 {
                NodeIndexError::ExhaustedSubIndices
            } else {
                NodeIndexError::ExhaustedSubSubIndices
            });
        }
        rows.sort_unstable();
        self.ast.keys = RowKeys::Annotation {
            first,
            rows: rows.into_boxed_slice(),
        };
        Ok(())
    }

    /// Recovered coordinates of expressions inside a decoded string annotation refer to
    /// the containing string. Identifier and literal-part spans still describe the decoded text.
    pub fn relocate_expression(&mut self, range: TextRange) {
        for row in &mut self.ast.rows {
            let kind = row.kind();
            if crate::kind::Expr::matches(kind)
                || matches!(kind, NodeKind::Arguments | NodeKind::Keyword)
            {
                row.set_range(range);
            }
        }
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for NodeKind {}

/// The row, source range and kind returned to the parser after emitting a node.
pub type EmittedNode<T> = (BuildId<T>, TextRange, NodeKind);

/// Initial field values for a single row. All child IDs already belong to the builder.
pub trait Emit {
    type Output: NodeType;

    /// Returns the handle, range and kind used by the parser's next grammar production.
    fn emit(self, builder: &mut Builder) -> EmittedNode<Self::Output>;
}

#[inline]
fn offset(len: usize) -> u32 {
    u32::try_from(len)
        .ok()
        .filter(|n| *n != ABSENT)
        .expect("Python AST exceeds the 32-bit storage limit")
}
