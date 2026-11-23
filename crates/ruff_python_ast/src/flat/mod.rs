//! Python syntax stored in a table of fixed-size rows with four-byte child handles.
//!
//! A handle identifies a row in one AST. A view borrows that AST to resolve the
//! handle's fields. Growing the backing vectors does not invalidate handles.
//!
//! The builder validates each child slot, child list and interned name before
//! appending the parent row. A child precedes its parent, and each list already
//! occupies a slice of the extra-data arena. A rewind restores lengths saved
//! together by this builder, so these references stay valid in the retained rows.
//! Typed getters can rely on those checks for the lifetime of a shared AST borrow.
//! When borrowing a concrete node from an external handle, we check the slot and
//! its kind before interpreting the row as that layout.

use std::fmt;
use std::hash::BuildHasher;
use std::marker::PhantomData;
use std::num::NonZeroU32;

use hashbrown::HashTable;
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxBuildHasher;
use thin_vec::ThinVec;

use crate::kind::Any;
use crate::{MAX_REAL_INDEX, NodeIndex, NodeIndexError, NodeKind, sub_ast_level, sub_indices};

mod children;
mod generated;
mod list;
mod literal;
mod nodes;
mod rewrite;
mod row;
mod scalar;
mod storage;
use storage::NodeStorage;
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
    ArgumentId, FStringPartId, FStringPartView, FilteredNodes, Iter as ListIter, List, ListId,
    ListItem, MappingPatternItemId, MappingPatternItems, MappingPatternItemsId, ParameterItemId,
    ParameterItems, ParameterItemsId, PatternArgumentId,
};
pub use nodes::{
    AnyParameter, ArgOrKeyword, ArgumentsSourceOrder, BytesValue, DebugText, FStringValue,
    LiteralExpression, StringValue, TStringValue,
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

    const fn cast<U>(self) -> BuildId<U> {
        BuildId::new(self.slot)
    }
}

// Share one failure path across the inlined child lookups.
#[cold]
#[inline(never)]
fn invalid_child() -> ! {
    panic!("invalid AST child slot")
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

    pub trait FromSyntax<'ast> {
        /// # Safety
        ///
        /// For a concrete node type, the stored row's kind must match `Self`.
        #[expect(unsafe_code)]
        unsafe fn from_syntax(syntax: SyntaxNode<'ast>) -> Self;
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

    /// Zero-based allocation index of this node in its AST.
    fn row_index(self) -> usize {
        self.syntax_node().slot_usize() - 1
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
        let syntax = self.syntax_node();
        syntax.ast.direct_children(syntax, self.node_kind())
    }

    /// Visits this node and its descendants in source order, parents first.
    fn descendants(self) -> Descendants<'ast> {
        self.syntax_node().descendants()
    }
}

impl<'ast> SyntaxNode<'ast> {
    #[inline]
    fn from_slot(ast: &'ast Ast, slot: usize) -> Self {
        debug_assert_ne!(slot, 0);
        Self {
            ast,
            row: ast
                .nodes
                .rows()
                .get(slot)
                .unwrap_or_else(|| invalid_child()),
        }
    }

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Builder::push checks that every row index fits in a u32."
    )]
    fn slot(self) -> u32 {
        self.slot_usize() as u32
    }

    #[inline]
    fn slot_usize(self) -> usize {
        // SAFETY: The row belongs to this AST. Its distance from the beginning of the
        // allocation is the slot used by IDs and by child fields.
        #[expect(unsafe_code)]
        unsafe {
            std::ptr::from_ref(self.row).offset_from_unsigned(self.ast.nodes.rows().as_ptr())
        }
    }

    /// # Safety
    ///
    /// `slot` must index an initialized row of `ast`.
    #[inline]
    #[expect(unsafe_code)]
    unsafe fn from_slot_unchecked(ast: &'ast Ast, slot: u32) -> Self {
        // SAFETY: The caller guarantees the row exists in this arena.
        let row = unsafe { ast.nodes.rows().get_unchecked(slot as usize) };
        Self { row, ast }
    }

    /// # Safety
    ///
    /// `slot` must come from a present child field of type `N` in this row's layout.
    #[inline]
    #[expect(unsafe_code)]
    unsafe fn stored_child<N: AstNode<'ast>>(self, slot: u32) -> N {
        // SAFETY: Builder::validate_child checked the slot and concrete layout before
        // appending this parent. Rewind and replacement can only remove or replace
        // the child after removing its parent.
        unsafe { N::from_syntax(Self::from_slot_unchecked(self.ast, slot)) }
    }

    /// # Safety
    ///
    /// `slot` must come from an optional child field of type `N` in this row's layout.
    #[inline]
    #[expect(unsafe_code)]
    unsafe fn stored_optional_child<N: AstNode<'ast>>(self, slot: u32) -> Option<N> {
        // SAFETY: A nonzero slot means this field is present and was validated.
        (slot != 0).then(|| unsafe { self.stored_child(slot) })
    }

    /// # Safety
    ///
    /// `id` must come from a name field in this row's layout.
    #[inline]
    #[expect(unsafe_code)]
    unsafe fn stored_name(self, id: u32) -> &'ast str {
        // SAFETY: The builder checked the pool index when it wrote the field. Rewind
        // restores an earlier state in which every retained row had its name.
        unsafe { self.ast.names.at_unchecked(id as usize) }
    }

    fn retype<N: AstNode<'ast>>(self) -> N {
        // A build ID or list can come from another arena or refer to a reused slot.
        // Check the layout before letting typed getters interpret words as stored IDs.
        if let Some(kind) = N::Kind::KIND
            && self.row.kind() != kind
        {
            invalid_child();
        }
        // SAFETY: For concrete types the stored kind matches. Group wrappers
        // inspect the actual kind before dispatching to a concrete node.
        #[expect(unsafe_code)]
        unsafe {
            N::from_syntax(self)
        }
    }

    /// Yields this node's immediate children in source order.
    #[inline]
    pub fn children(self) -> Children<'ast> {
        AstNode::children(self)
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
    #[expect(unsafe_code)]
    unsafe fn from_syntax(syntax: Self) -> Self {
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
        // SAFETY: The row belongs to this arena. Its range is inserted, removed and
        // truncated together with the row. A shared arena borrow fixes both lengths.
        #[expect(unsafe_code)]
        unsafe {
            *self.ast.nodes.ranges().get_unchecked(self.slot_usize())
        }
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
            #[expect(unsafe_code)]
            unsafe fn from_syntax(syntax: crate::SyntaxNode<'ast>) -> Self {
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
#[derive(Clone, Debug)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
// Resolving a child ID loads the row pointer from `nodes`. Keep `nodes` first
// so x86-64 can encode that load with a short displacement from the AST pointer.
#[repr(C)]
pub struct Ast {
    nodes: NodeStorage,
    extra: Vec<u32>,
    text: String,
    /// One copy of each distinct spelling, indexed by AST-local [`NameId`].
    /// Only the builder needs the reverse lookup from spelling to ID.
    ///
    /// Packing the UTF-8 bytes together lets us borrow names during a walk
    /// and drop them all without visiting individual entries. Consumers such
    /// as ty copy a spelling into a [`Name`](crate::name::Name) when they need
    /// to retain it independently of the AST.
    names: NamePool,
    // Most files have no bytes literals. Keep only a pointer in the AST and store
    // the length and capacity alongside the bytes when the buffer is allocated.
    bytes: ThinVec<u8>,
    keys: RowKeys,
}

impl Default for Ast {
    fn default() -> Self {
        Self::empty()
    }
}

impl Ast {
    const fn empty() -> Self {
        Self {
            nodes: NodeStorage::empty(),
            extra: Vec::new(),
            text: String::new(),
            names: NamePool::empty(),
            bytes: ThinVec::new(),
            keys: RowKeys::Unindexed,
        }
    }

    /// Whether an identifier with this normalized spelling occurs in the tree.
    /// This scans the distinct names interned during parsing.
    pub fn contains_identifier(&self, spelling: &str) -> bool {
        (0..self.names.len()).any(|i| self.names.at(i) == spelling)
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
        let row = self.nodes.rows().get(handle.slot())?;
        if !T::ANY && !T::matches(row.kind()) {
            return None;
        }
        Some(SyntaxNode { ast: self, row }.retype())
    }

    /// Borrows an untyped row by its allocation index.
    pub fn row(&self, index: u32) -> Option<SyntaxNode<'_>> {
        Some(SyntaxNode {
            ast: self,
            row: self.nodes.rows().get((index as usize).checked_add(1)?)?,
        })
    }

    /// Number of stored nodes, excluding the reserved slot.
    pub fn len(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Searches the stored nodes, including any that parser recovery leaves outside the tree.
    pub fn any_node<'ast>(&'ast self, predicate: impl FnMut(SyntaxNode<'ast>) -> bool) -> bool {
        self.nodes
            .rows()
            .iter()
            .skip(1)
            .map(|row| SyntaxNode { ast: self, row })
            .any(predicate)
    }

    #[inline]
    fn list_words(&self, word: u64) -> &[u32] {
        self.extra
            .get(Interval::decode(word).indices())
            .unwrap_or_else(|| invalid_child())
    }

    #[inline]
    fn validate_list(&self, word: u64) {
        let items = self.list_words(word);
        let len = self.nodes.len();
        if items.iter().any(|&slot| slot as usize >= len) {
            invalid_child();
        }
    }

    fn at<T: NodeType>(&self, handle: AstNodeId<T>) -> T::Borrowed<'_> {
        SyntaxNode::from_slot(self, handle.slot()).retype()
    }

    fn child<'ast, N: AstNode<'ast>>(&'ast self, field: u64) -> N {
        SyntaxNode::from_slot(self, split_words(field)[0] as usize).retype()
    }

    fn bytes(&self, field: u64) -> &[u8] {
        &self.bytes[Interval::decode(field).indices()]
    }

    fn string(&self, field: u64) -> &str {
        &self.text[Interval::decode(field).indices()]
    }
}

/// Consecutive offsets delimit each spelling within `text`. For example,
/// interning `"x"` and `"foo"` stores `"xfoo"` with offsets `[0, 1, 4]`.
/// Keeping the bytes separate from string literals saves a length per name.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
struct NamePool {
    offsets: Vec<u32>,
    text: String,
}

impl NamePool {
    const fn empty() -> Self {
        Self {
            offsets: Vec::new(),
            text: String::new(),
        }
    }

    fn with_capacity(count: usize) -> Self {
        if count == 0 {
            return Self::empty();
        }
        Self {
            offsets: Vec::with_capacity(count + 1),
            // Estimate eight UTF-8 bytes per distinct spelling.
            text: String::with_capacity(count * 8),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    #[inline]
    fn push(&mut self, spelling: &str) {
        let end = offset(self.text.len() + spelling.len());
        if self.offsets.is_empty() {
            // Delay the first boundary so empty inputs need no name storage.
            self.offsets.push(0);
        }
        self.text.push_str(spelling);
        self.offsets.push(end);
    }

    /// # Safety
    ///
    /// `index` must be less than `self.len()`.
    #[inline]
    #[expect(unsafe_code)]
    unsafe fn at_unchecked(&self, index: usize) -> &str {
        // SAFETY: Both offsets exist for an in-bounds index. Each push records
        // the end of a complete UTF-8 spelling, and truncation keeps the text
        // through the last retained offset. Equal offsets denote an empty name.
        unsafe {
            let start = *self.offsets.get_unchecked(index) as usize;
            let end = *self.offsets.get_unchecked(index + 1) as usize;
            self.text.get_unchecked(start..end)
        }
    }

    #[inline]
    fn at(&self, index: usize) -> &str {
        assert!(index < self.len());
        // SAFETY: The index is in bounds.
        #[expect(unsafe_code)]
        unsafe {
            self.at_unchecked(index)
        }
    }

    fn truncate(&mut self, len: usize) {
        if len < self.len() {
            self.text.truncate(self.offsets[len] as usize);
            self.offsets.truncate(len + 1);
        }
    }

    fn shrink_to_fit(&mut self) {
        self.offsets.shrink_to_fit();
        self.text.shrink_to_fit();
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
    checkpoints: Vec<ArenaLengths>,
}

/// The index of an identifier spelling in one builder's name pool.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NameId(u32);

impl NameId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A saved builder state before a speculative parse.
#[must_use = "commit or rewind the checkpoint to release it from the builder"]
pub struct Checkpoint {
    index: usize,
    lengths: ArenaLengths,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArenaLengths {
    rows: usize,
    extra: usize,
    text: usize,
    names: usize,
    bytes: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Self::for_source(TextRange::default())
    }
}

impl Builder {
    /// Start with a small sample of rows so a file with little syntax needs little storage.
    pub fn for_source(source: TextRange) -> Self {
        const MAX_INITIAL_ROW_BYTES: usize = 8 * 1024;

        let bytes = source.len().to_usize();
        let estimated = (bytes / 18).min(4096);
        let row_limit = MAX_INITIAL_ROW_BYTES / (size_of::<Row>() + size_of::<TextRange>()) - 1;
        let estimated_rows = estimated.min(row_limit);
        // A small expression such as `x | y` needs three rows in addition to the sentinel.
        let mut nodes = NodeStorage::with_capacity((estimated_rows + 1).max(4));
        // No view can refer to this row. Its presence makes every child slot a nonzero index.
        assert!(nodes.try_push(Row::new(NodeKind::ModModule), TextRange::default()));
        Self {
            source,
            ast: Ast {
                nodes,
                extra: Vec::with_capacity(estimated + estimated / 8),
                text: String::with_capacity((bytes / 8).min(4096)),
                names: NamePool::with_capacity(estimated / 8),
                ..Ast::default()
            },
            checkpoints: Vec::new(),
            name_ids: HashTable::with_capacity(estimated * 3 / 8),
            scratch: Vec::new(),
        }
    }

    /// Turns a name expression into the identifier for a keyword argument. When the name is
    /// the last row, its expression handle expires and the identifier can reuse that row.
    pub fn take_name(
        &mut self,
        name: BuildId<crate::kind::ExprName>,
    ) -> BuildId<crate::kind::Identifier> {
        let slot = name.slot();
        let mut identifier = Row::new(NodeKind::Identifier);
        // Replacing the last row keeps its already-validated name in the same pool.
        // All earlier rows only point backward, so none depends on this row's kind.
        identifier.set_identifier_id(self.get(name).name_id().0);
        if slot + 1 == self.ast.nodes.len() {
            self.ast.nodes.rows_mut()[slot] = identifier;
            name.cast()
        } else {
            self.push(identifier, self.ast.nodes.ranges()[slot])
        }
    }

    /// Discards a temporary pattern after its child becomes a dictionary key or class keyword.
    /// Handles to the wrapper expire if it is the final arena row.
    pub fn discard_pattern_wrapper(&mut self, pattern: BuildId<crate::kind::Pattern>) {
        self.discard_if_last(pattern);
    }

    /// In `case Point()`, the parser recognizes a capture before discovering that its name
    /// belongs to the class expression. Keep the name and span and release both temporary rows.
    pub fn take_pattern_name(
        &mut self,
        pattern: BuildId<crate::kind::Pattern>,
        name: BuildId<crate::kind::Identifier>,
    ) -> build::Identifier {
        let row = &self.ast.nodes.rows()[name.slot()];
        let identifier = build::Identifier {
            range: self.ast.nodes.ranges()[name.slot()],
            id: NameId(row.identifier_id()),
        };
        self.discard_if_last(pattern);
        self.discard_if_last(name);
        identifier
    }

    fn discard_if_last<T>(&mut self, id: BuildId<T>) {
        if id.slot() + 1 == self.ast.nodes.len() {
            while self
                .checkpoints
                .last()
                .is_some_and(|saved| saved.rows > id.slot())
            {
                self.checkpoints.pop();
            }
            self.ast.nodes.truncate(id.slot());
        }
    }

    /// Saves the lengths of the rows and side tables. Call [`Self::commit`] or
    /// [`Self::rewind`] to release the saved state and any nested checkpoints.
    pub fn checkpoint(&mut self) -> Checkpoint {
        let lengths = ArenaLengths {
            rows: self.ast.nodes.len(),
            extra: self.ast.extra.len(),
            text: self.ast.text.len(),
            names: self.ast.names.len(),
            bytes: self.ast.bytes.len(),
        };
        let index = self.checkpoints.len();
        self.checkpoints.push(lengths);
        Checkpoint { index, lengths }
    }

    /// Releases this checkpoint and any nested checkpoints, keeping all allocations.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A checkpoint is consumed on commit."
    )]
    pub fn commit(&mut self, checkpoint: Checkpoint) {
        // Restoring a recorded set of lengths preserves every retained row's
        // references. Lengths from another arena could drop a name or list
        // while retaining the row that points to it.
        assert_eq!(
            self.checkpoints.get(checkpoint.index),
            Some(&checkpoint.lengths),
            "invalid AST checkpoint"
        );
        self.checkpoints.truncate(checkpoint.index);
    }

    /// Discards allocations since the checkpoint. Handles to those allocations expire here.
    pub fn rewind(&mut self, checkpoint: Checkpoint) {
        let lengths = checkpoint.lengths;
        self.commit(checkpoint);
        self.ast.nodes.truncate(lengths.rows);
        self.ast.extra.truncate(lengths.extra);
        self.ast.text.truncate(lengths.text);
        // Removing an ID requires hashing its spelling while its bytes are still available.
        for i in lengths.names..self.ast.names.len() {
            let name = self.ast.names.at(i);
            let index = offset(i);
            if let Ok(entry) = self
                .name_ids
                .find_entry(FxBuildHasher.hash_one(name), |&id| id == index)
            {
                entry.remove();
            }
        }
        self.ast.names.truncate(lengths.names);
        self.ast.bytes.truncate(lengths.bytes);
    }

    #[inline]
    fn validate_child<T: NodeType, const OPTIONAL: bool>(&self, slot: u32) {
        // A child already exists when its parent is appended. Slot zero is the
        // sentinel used by an absent optional child.
        let Some(row) = self.ast.nodes.rows().get(slot as usize) else {
            invalid_child();
        };
        if let Some(kind) = T::KIND
            && (!OPTIONAL || slot != 0)
            && row.kind() != kind
        {
            invalid_child();
        }
    }

    #[inline]
    fn validate_name(&self, id: u32) {
        if id as usize >= self.ast.names.len() {
            invalid_child();
        }
    }

    fn node_count(&self) -> usize {
        self.ast.nodes.len() - 1
    }

    pub fn finish_tree<T>(self, root: BuildId<T>) -> Tree<T> {
        Tree {
            ast: Box::new(self.ast),
            root: AstNodeId::from_word(root.slot),
        }
    }

    pub fn get<T: NodeType>(&self, id: BuildId<T>) -> T::Borrowed<'_> {
        let syntax = SyntaxNode::from_slot(&self.ast, id.slot());
        let node = syntax.retype();
        debug_assert!(T::matches(syntax.row.kind()));
        node
    }

    /// The source range of a node in this builder.
    #[inline]
    pub fn range<T>(&self, id: BuildId<T>) -> TextRange {
        self.ast.nodes.ranges()[id.slot()]
    }

    /// Adds a node whose child IDs already belong to this builder.
    pub fn add<N: Emit>(&mut self, node: N) -> BuildId<N::Output> {
        node.emit(self)
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

    #[cfg(test)]
    fn get_list<E: list::Element>(&self, list: ListId<E>) -> List<'_, E> {
        List::new(&self.ast, list.word())
    }

    /// Sets the load, store or delete context of an assignment target and its elements.
    pub fn set_expr_context(
        &mut self,
        expr: BuildId<crate::kind::Expr>,
        context: crate::ExprContext,
    ) {
        fn set(rows: &mut [Row], extra: &[u32], slot: u32, context: u8) {
            let row = &mut rows[slot as usize];
            let elements = match row.kind() {
                NodeKind::ExprName => return row.set_expr_name_ctx(context),
                NodeKind::ExprAttribute => return row.set_expr_attribute_ctx(context),
                NodeKind::ExprSubscript => return row.set_expr_subscript_ctx(context),
                NodeKind::ExprStarred => {
                    row.set_expr_starred_ctx(context);
                    let child = row.expr_starred_value();
                    return set(rows, extra, child, context);
                }
                NodeKind::ExprUnaryOp => {
                    let child = row.expr_unary_op_operand();
                    return set(rows, extra, child, context);
                }
                NodeKind::ExprTuple => {
                    row.set_expr_tuple_ctx(context);
                    row.expr_tuple_elts()
                }
                NodeKind::ExprList => {
                    row.set_expr_list_ctx(context);
                    row.expr_list_elts()
                }
                _ => return,
            };
            for &child in &extra[Interval::decode(elements).indices()] {
                set(rows, extra, child, context);
            }
        }

        let context = context.encode().to_le_bytes()[0];
        let rows = self.ast.nodes.rows_mut();
        let row = &mut rows[expr.slot()];
        match row.kind() {
            NodeKind::ExprName => row.set_expr_name_ctx(context),
            NodeKind::ExprAttribute => row.set_expr_attribute_ctx(context),
            NodeKind::ExprSubscript => row.set_expr_subscript_ctx(context),
            _ => set(rows, &self.ast.extra, expr.slot, context),
        }
    }

    #[inline]
    fn push<T: NodeType>(&mut self, row: Row, range: TextRange) -> BuildId<T> {
        let index = offset(self.ast.nodes.len());
        if !self.ast.nodes.try_push(row, range) {
            self.grow_and_push(row, range);
        }
        BuildId::new(index)
    }

    #[cold]
    fn grow_and_push(&mut self, row: Row, range: TextRange) {
        if self.source.is_empty() {
            self.ast
                .nodes
                .reserve_exact(self.ast.nodes.capacity().max(4));
        } else {
            self.reserve_rows(range.end());
        }
        assert!(self.ast.nodes.try_push(row, range));
    }

    fn reserve_rows(&mut self, end: TextSize) {
        // A module can start with a long docstring or a license header. Allow its remaining
        // source to be denser than the prefix, and leave some room for variation after it.
        // Conversely, a dense prefix followed by a long comment needs few more rows. Limit
        // speculative reservations to one MiB per buffer. Once a large row buffer fills,
        // keep growing it geometrically so repeated reallocations stay amortized.
        const MAX_SPECULATIVE_BYTES: usize = 1 << 20;

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
        let minimum_growth = rows.div_ceil(4).max(4);
        let additional = estimate
            .saturating_add(estimate / 8)
            .saturating_sub(rows)
            .min(rows.saturating_mul(128))
            .min(MAX_SPECULATIVE_BYTES / (size_of::<Row>() + size_of::<TextRange>()))
            .max(minimum_growth);
        self.ast.nodes.reserve_exact(additional);
        let extra = self
            .ast
            .extra
            .len()
            .saturating_mul(additional)
            .checked_div(rows)
            .unwrap_or(0)
            .min(MAX_SPECULATIVE_BYTES / size_of::<u32>());
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
    ///
    /// Validates `parts` for the generated getter before caching its joined text.
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

    fn join_string_literals(&mut self, word: u64) -> Interval {
        // List::new checks the interval and every child slot before the parent stores them.
        let parts = List::<AstNodeId<crate::kind::StringLiteral>>::new(&self.ast, word);
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
            for index in 0..parts.len() {
                let range = self.list_item(parts, index).text_range();
                self.ast.text.extend_from_within(range.indices());
            }
            Interval::new(start, self.ast.text.len() - start)
        }
    }

    pub fn intern_name(&mut self, text: &str) -> NameId {
        let names = &mut self.ast.names;
        let hash = FxBuildHasher.hash_one(text);
        if let Some(&index) = self
            .name_ids
            .find(hash, |&index| names.at(index as usize) == text)
        {
            return NameId(index);
        }
        let index = offset(names.len());
        names.push(text);
        self.name_ids.insert_unique(hash, index, |&index| {
            FxBuildHasher.hash_one(names.at(index as usize))
        });
        NameId(index)
    }

    pub fn name(&self, id: NameId) -> &str {
        self.ast.names.at(id.index())
    }

    fn store_debug_text(&mut self, parts: build::DebugText<'_>) -> u32 {
        let text = &mut self.ast.text;
        text.reserve(parts.leading.len() + parts.expression.len() + parts.trailing.len());
        let start = offset(text.len());
        text.push_str(parts.leading);
        let expression_start = offset(text.len());
        text.push_str(parts.expression);
        let expression_end = offset(text.len());
        text.push_str(parts.trailing);
        let end = offset(text.len());

        let slot = offset(self.ast.extra.len());
        self.ast
            .extra
            .extend([start, expression_start, expression_end, end]);
        slot
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
        self.finish_list(start)
    }

    fn finish_list<E>(&mut self, start: usize) -> ListId<E> {
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
        U::matches(self.ast.nodes.rows()[self.root.slot()].kind()).then(|| Tree {
            root: self.root.cast(),
            ast: self.ast,
        })
    }

    /// Releases spare capacity in the row and side-table buffers.
    pub fn shrink_to_fit(&mut self) {
        self.ast.nodes.shrink_to_fit();
        self.ast.extra.shrink_to_fit();
        self.ast.text.shrink_to_fit();
        self.ast.names.shrink_to_fit();
        self.ast.bytes.shrink_to_fit();
    }

    fn required_node_indices(&self) -> usize {
        let count = self.ast.nodes.len() - 1;
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
        let (rows, ranges) = self.ast.nodes.rows_and_ranges_mut();
        for (row, span) in rows.iter().zip(ranges) {
            let kind = row.kind();
            if crate::kind::Expr::matches(kind)
                || matches!(kind, NodeKind::Arguments | NodeKind::Keyword)
            {
                *span = range;
            }
        }
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for NodeKind {}

/// Initial field values for a single row. All child IDs already belong to the builder.
pub trait Emit {
    type Output: NodeType;

    /// Inserts the node into the builder and returns its ID.
    fn emit(self, builder: &mut Builder) -> BuildId<Self::Output>;
}

#[inline]
fn offset(len: usize) -> u32 {
    u32::try_from(len)
        .ok()
        .filter(|n| *n != ABSENT)
        .expect("Python AST exceeds the 32-bit storage limit")
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::{BuildId, Builder, NodeKind, Row, TextRange, TextSize, build};
    use crate::{ExprContext, ModModule, kind};

    #[test]
    fn dense_prefix_bounds_speculative_reservations() {
        let source = TextRange::up_to(TextSize::new(8 << 20));
        let mut builder = Builder::for_source(source);
        builder.ast.nodes.reserve_exact(16_384);
        for _ in 0..16_384 {
            assert!(
                builder
                    .ast
                    .nodes
                    .try_push(Row::new(NodeKind::StmtPass), TextRange::default())
            );
        }
        builder.ast.extra.resize(128 << 10, 0);

        builder.reserve_rows(TextSize::new(32 << 10));
        let nodes = &builder.ast.nodes;
        let extra = &builder.ast.extra;
        let spare_bytes =
            (nodes.capacity() - nodes.len()) * (size_of::<Row>() + size_of::<TextRange>());
        assert!(spare_bytes <= 1 << 20);
        assert!((extra.capacity() - extra.len()) * size_of::<u32>() <= 1 << 20);
    }

    fn none(builder: &mut Builder) -> BuildId<kind::Expr> {
        builder
            .add(build::ExprNoneLiteral {
                range: TextRange::default(),
            })
            .into()
    }

    fn statement(builder: &mut Builder, spelling: &str) -> BuildId<kind::StmtExpr> {
        let range = TextRange::default();
        let id = builder.intern_name(spelling);
        let value = builder
            .add(build::ExprName {
                range,
                ctx: ExprContext::Load,
                id,
            })
            .into();
        builder.add(build::StmtExpr { range, value })
    }

    fn module(builder: &mut Builder, spelling: &str) -> BuildId<kind::ModModule> {
        let item: BuildId<kind::Stmt> = statement(builder, spelling).into();
        let body = builder.list([item]);
        builder.add(build::ModModule {
            range: TextRange::default(),
            body,
        })
    }

    fn spelling(module: ModModule<'_>) -> &str {
        module
            .body()
            .at(0)
            .as_expr_stmt()
            .unwrap()
            .value()
            .as_name_expr()
            .unwrap()
            .id()
    }

    #[test]
    #[should_panic(expected = "invalid AST child slot")]
    fn reject_stale_child_before_storing_parent() {
        let mut builder = Builder::default();
        let checkpoint = builder.checkpoint();
        let value = none(&mut builder);
        builder.rewind(checkpoint);
        builder.add(build::StmtExpr {
            range: TextRange::default(),
            value,
        });
    }

    #[test]
    #[should_panic(expected = "invalid AST child slot")]
    fn reject_foreign_concrete_child_with_different_local_kind() {
        let range = TextRange::default();
        let mut other = Builder::default();
        let id = other.intern_name("other");
        let name = other.add(build::Identifier { range, id });
        let mut builder = Builder::default();
        none(&mut builder);
        builder.add(build::Alias {
            range,
            name,
            asname: None,
        });
    }

    #[test]
    #[should_panic(expected = "invalid AST child slot")]
    fn reject_name_outside_local_pool() {
        let id = Builder::default().intern_name("other");
        Builder::default().add(build::Identifier {
            range: TextRange::default(),
            id,
        });
    }

    #[test]
    fn names_borrow_distinct_spellings() {
        let mut builder = Builder::default();
        let first = builder.intern_name("π");
        let empty = builder.intern_name("");
        let last = builder.intern_name("connection_timeout_seconds");
        assert_eq!(builder.intern_name("π"), first);
        assert_eq!(builder.intern_name(""), empty);
        assert_eq!(builder.name(empty), "");
        assert_eq!(builder.name(last), "connection_timeout_seconds");

        let root = builder.add(build::Identifier {
            id: first,
            range: TextRange::default(),
        });
        let tree = builder.finish_tree(root);
        assert_eq!(tree.root().as_str(), "π");
        assert!(tree.ast().contains_identifier(""));
        assert!(tree.ast().contains_identifier("connection_timeout_seconds"));
        assert!(
            !tree
                .ast()
                .contains_identifier("πconnection_timeout_seconds")
        );
    }

    #[test]
    fn rewind_reuses_name_ids_with_local_boundaries() {
        let mut builder = Builder::default();
        let first = builder.intern_name("π");
        let checkpoint = builder.checkpoint();
        let removed = builder.intern_name("connection_timeout_seconds");
        builder.rewind(checkpoint);
        assert_eq!(builder.intern_name("π"), first);
        let replacement = builder.intern_name("x");
        assert_eq!(replacement, removed);
        let restored = builder.intern_name("connection_timeout_seconds");
        assert_ne!(replacement, restored);
        assert_eq!(builder.name(replacement), "x");
        assert_eq!(builder.name(restored), "connection_timeout_seconds");
    }

    #[test]
    #[should_panic(expected = "invalid AST child slot")]
    fn reject_list_outside_local_arena() {
        let mut other = Builder::default();
        let item: BuildId<kind::Stmt> = statement(&mut other, "other").into();
        let body = other.list([item]);
        Builder::default().add(build::ModModule {
            range: TextRange::default(),
            body,
        });
    }

    #[test]
    #[should_panic(expected = "invalid AST child slot")]
    fn reject_list_containing_out_of_bounds_slot() {
        let item: BuildId<kind::Stmt> = statement(&mut Builder::default(), "other").into();
        let mut builder = Builder::default();
        let body = builder.list([item]);
        builder.add(build::ModModule {
            range: TextRange::default(),
            body,
        });
    }

    #[test]
    fn reject_reused_slot_at_each_typed_entry_point() {
        let mut builder = Builder::default();
        let checkpoint = builder.checkpoint();
        let stale = statement(&mut builder, "old");
        builder.rewind(checkpoint);
        none(&mut builder);
        none(&mut builder);
        let list = builder.list([stale]);
        assert!(catch_unwind(|| builder.get(stale)).is_err());
        assert!(catch_unwind(|| builder.get_list(list).at(0)).is_err());
        let tree = builder.finish_tree(stale);
        assert!(catch_unwind(|| tree.root()).is_err());
    }

    #[test]
    fn reject_foreign_checkpoint_with_matching_row_count() {
        let mut other = Builder::default();
        for _ in 0..3 {
            none(&mut other);
        }
        let foreign = other.checkpoint();
        let mut builder = Builder::default();
        let retained = module(&mut builder, "kept");
        let local = builder.checkpoint();
        // The foreign lengths would retain the module but delete its list and name.
        assert!(catch_unwind(AssertUnwindSafe(|| builder.rewind(foreign))).is_err());
        builder.rewind(local);
        assert_eq!(spelling(builder.get(retained)), "kept");
    }

    #[test]
    fn discard_invalidates_checkpoints_that_included_the_row() {
        let mut builder = Builder::default();
        let value = none(&mut builder);
        let stale = builder.checkpoint();
        builder.discard_if_last(value);
        let retained = module(&mut builder, "replacement");
        assert!(catch_unwind(AssertUnwindSafe(|| builder.rewind(stale))).is_err());
        assert_eq!(spelling(builder.get(retained)), "replacement");
    }

    #[test]
    fn nested_rewinds_preserve_the_retained_tree() {
        let mut builder = Builder::default();
        let root = module(&mut builder, "keep");
        let first = builder.checkpoint();
        module(&mut builder, "middle");
        let second = builder.checkpoint();
        for _ in 0..32 {
            module(&mut builder, "late");
        }
        builder.rewind(second);
        let third = builder.checkpoint();
        module(&mut builder, "committed");
        builder.commit(third);
        builder.rewind(first);
        module(&mut builder, "replacement");
        let mut tree = builder.finish_tree(root);
        tree.shrink_to_fit();
        let cloned = tree.clone();
        drop(tree);
        assert_eq!(spelling(cloned.root()), "keep");
    }
}
