use std::marker::PhantomData;
use std::ops::Bound;
use std::ops::Range;

use super::{
    Any, Ast, AstNode, AstNodeId, BuildId, Builder, NodeList, NodeType, SyntaxNode, offset,
    split_words,
};
use crate::{CmpOp, Expr, FString, Identifier, MappingPatternItem, Pattern, StringLiteral};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(super) struct Interval {
    start: u32,
    len: u32,
}

impl Interval {
    #[inline]
    pub(super) fn new(start: usize, len: usize) -> Self {
        Self {
            start: offset(start),
            len: offset(len),
        }
    }

    pub(super) fn encode(self) -> u64 {
        u64::from(self.start) | (u64::from(self.len) << 32)
    }

    pub(super) fn decode(word: u64) -> Self {
        let [start, len] = split_words(word);
        Self { start, len }
    }

    pub(super) fn indices(self) -> Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

const _: () = assert!(size_of::<Interval>() == 8);

/// Items stored in an AST list. The field schema determines how each word is read.
pub trait Element: Sized {
    const IS_NODE: bool = true;
    type Item<'a>: Copy;

    fn read(word: ListWord<'_, Self>) -> Self::Item<'_>;
}

/// A word and the arena of the list it came from. For node elements the word
/// is an in-bounds slot in this arena. Keeping them together prevents element
/// decoders from pairing a validated slot with a different AST.
pub struct ListWord<'a, E> {
    ast: &'a Ast,
    value: u32,
    element: PhantomData<fn() -> E>,
}

impl<'a, E: Element> ListWord<'a, E> {
    #[inline]
    fn node(self) -> SyntaxNode<'a> {
        const { assert!(E::IS_NODE) };
        // SAFETY: The word was checked in this arena, either individually by
        // Builder::list_item or as part of a validated List. The shared borrow
        // keeps the slot valid.
        #[expect(unsafe_code)]
        unsafe {
            SyntaxNode::from_slot_unchecked(self.ast, self.value)
        }
    }
}

/// A list descriptor that can be stored in a node under construction.
pub struct ListId<E> {
    range: Interval,
    marker: PhantomData<fn() -> E>,
}

impl<E> Copy for ListId<E> {}
impl<E> Clone for ListId<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E> std::fmt::Debug for ListId<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.range.fmt(f)
    }
}

impl<E> Default for ListId<E> {
    fn default() -> Self {
        Self::new(Interval { start: 0, len: 0 })
    }
}

impl<E> ListId<E> {
    pub(super) fn new(range: Interval) -> Self {
        Self {
            range,
            marker: PhantomData,
        }
    }

    pub(super) fn word(self) -> u64 {
        self.range.encode()
    }

    pub fn len(self) -> usize {
        self.range.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.range.len == 0
    }
}

impl Builder {
    /// Reads one list element, checking just its slot. Use this when building
    /// new nodes between reads, since each write ends the previous list borrow.
    pub fn list_item<E: Element>(&self, list: ListId<E>, index: usize) -> E::Item<'_> {
        let value = self.ast.extra[list.range.indices()][index];
        if E::IS_NODE && value as usize >= self.ast.nodes.len() {
            super::invalid_child();
        }
        E::read(ListWord {
            ast: &self.ast,
            value,
            element: PhantomData,
        })
    }
}

pub trait ListItem {
    type Element: Element;

    #[doc(hidden)]
    fn word(self) -> u32;
}

impl<T: NodeType + 'static> ListItem for AstNodeId<T> {
    type Element = Self;

    fn word(self) -> u32 {
        self.slot.get()
    }
}

impl<T: NodeType + 'static> ListItem for BuildId<T> {
    type Element = AstNodeId<T>;

    fn word(self) -> u32 {
        self.slot
    }
}

impl ListItem for CmpOp {
    type Element = Self;

    fn word(self) -> u32 {
        split_words(super::scalar::Word::encode(self))[0]
    }
}

/// The two kinds of children that can occupy a position in a call's argument list.
#[derive(Clone, Copy, Debug)]
pub enum ArgumentId {
    Positional(BuildId<crate::kind::Expr>),
    Keyword(BuildId<crate::kind::Keyword>),
}

impl From<ListId<AstNodeId<crate::kind::Expr>>> for ListId<ArgumentId> {
    fn from(positional: ListId<AstNodeId<crate::kind::Expr>>) -> Self {
        Self::new(positional.range)
    }
}

impl ListItem for ArgumentId {
    type Element = Self;

    fn word(self) -> u32 {
        match self {
            Self::Positional(expr) => expr.slot,
            Self::Keyword(keyword) => keyword.slot,
        }
    }
}

impl Element for ArgumentId {
    type Item<'a> = super::ArgOrKeyword<'a>;

    #[inline]
    fn read(word: ListWord<'_, Self>) -> Self::Item<'_> {
        let node = word.node();
        if let Some(keyword) = node.cast::<crate::Keyword>() {
            super::ArgOrKeyword::Keyword(keyword)
        } else {
            super::ArgOrKeyword::Arg(node.retype())
        }
    }
}

/// The node handles that can occupy a position in a class pattern's argument list.
pub type PatternArgumentId =
    crate::PatternOrKeyword<BuildId<crate::kind::Pattern>, BuildId<crate::kind::PatternKeyword>>;

impl ListItem for PatternArgumentId {
    type Element = Self;

    fn word(self) -> u32 {
        match self {
            Self::Pattern(pattern) => pattern.slot,
            Self::Keyword(keyword) => keyword.slot,
        }
    }
}

impl Element for PatternArgumentId {
    type Item<'a> = crate::PatternOrKeyword<crate::Pattern<'a>, crate::PatternKeyword<'a>>;

    #[inline]
    fn read(word: ListWord<'_, Self>) -> Self::Item<'_> {
        let node = word.node();
        if let Some(keyword) = node.cast::<crate::PatternKeyword>() {
            crate::PatternOrKeyword::Keyword(keyword)
        } else {
            crate::PatternOrKeyword::Pattern(node.retype())
        }
    }
}

/// The children of one type in a heterogeneous, source-ordered AST list.
pub struct FilteredNodes<'a, T> {
    nodes: NodeList<'a, Any>,
    marker: PhantomData<fn() -> T>,
}

impl<'a, T: NodeType> std::fmt::Debug for FilteredNodes<'a, T>
where
    T::Borrowed<'a>: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(*self).finish()
    }
}

impl<T> Copy for FilteredNodes<'_, T> {}
impl<T> Clone for FilteredNodes<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: NodeType> FilteredNodes<'a, T> {
    pub(super) fn new<E: Element>(items: List<'a, E>) -> Self {
        const { assert!(E::IS_NODE) };
        Self {
            nodes: List {
                ast: items.ast,
                range: items.range,
                element: PhantomData,
            },
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter(self) -> FilteredNodeIter<'a, T> {
        FilteredNodeIter {
            nodes: self.nodes.iter(),
            marker: PhantomData,
        }
    }
    pub fn is_empty(self) -> bool {
        self.first().is_none()
    }
    pub fn len(self) -> usize {
        self.iter().count()
    }
    pub fn get(self, index: usize) -> Option<T::Borrowed<'a>> {
        self.iter().nth(index)
    }

    pub fn split_first(self) -> Option<(T::Borrowed<'a>, Self)> {
        let mut iter = self.iter();
        let first = iter.next()?;
        Some((first, Self::new(iter.nodes.as_slice())))
    }

    #[must_use]
    pub fn skip_front(self, n: usize) -> Self {
        let mut iter = self.iter();
        if n > 0 {
            iter.nth(n - 1);
        }
        Self::new(iter.nodes.as_slice())
    }

    pub fn first(self) -> Option<T::Borrowed<'a>> {
        self.iter().next()
    }
    pub fn last(self) -> Option<T::Borrowed<'a>> {
        self.iter().next_back()
    }

    pub fn as_array<const N: usize>(self) -> Option<[T::Borrowed<'a>; N]> {
        if self.nodes.len() < N {
            return None;
        }
        itertools::Itertools::collect_array(self.iter())
    }
}

impl<'a, T: NodeType> IntoIterator for FilteredNodes<'a, T> {
    type Item = T::Borrowed<'a>;
    type IntoIter = FilteredNodeIter<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct FilteredNodeIter<'a, T> {
    nodes: Iter<'a, AstNodeId<Any>>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for FilteredNodeIter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            marker: PhantomData,
        }
    }
}

impl<'a, T: NodeType> Iterator for FilteredNodeIter<'a, T> {
    type Item = T::Borrowed<'a>;
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.nodes.len()))
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
    fn next(&mut self) -> Option<Self::Item> {
        self.nodes.find_map(AstNode::cast::<T::Borrowed<'a>>)
    }
}

impl<'a, T: NodeType> From<NodeList<'a, T>> for FilteredNodes<'a, T> {
    fn from(nodes: NodeList<'a, T>) -> Self {
        Self::new(nodes)
    }
}

impl<'a, T: NodeType> DoubleEndedIterator for FilteredNodeIter<'a, T> {
    fn next_back(&mut self) -> Option<T::Borrowed<'a>> {
        self.nodes
            .by_ref()
            .rev()
            .find_map(AstNode::cast::<T::Borrowed<'a>>)
    }
}

impl<T: NodeType> std::iter::FusedIterator for FilteredNodeIter<'_, T> {}

impl<'a, T: NodeType> Default for FilteredNodes<'a, T> {
    fn default() -> Self {
        Self::new(NodeList::<'a, T>::default())
    }
}

pub enum FStringPartId {
    Literal(BuildId<crate::kind::StringLiteral>),
    FString(BuildId<crate::kind::FString>),
}

impl ListItem for FStringPartId {
    type Element = crate::FStringPart;

    fn word(self) -> u32 {
        match self {
            Self::Literal(value) => value.slot,
            Self::FString(value) => value.slot,
        }
    }
}

impl<T: NodeType + 'static> Element for AstNodeId<T> {
    type Item<'a> = T::Borrowed<'a>;

    #[inline]
    fn read(word: ListWord<'_, Self>) -> Self::Item<'_> {
        word.node().retype()
    }
}

impl Element for CmpOp {
    const IS_NODE: bool = false;
    type Item<'a> = Self;

    fn read(word: ListWord<'_, Self>) -> Self {
        super::scalar::Word::decode(u64::from(word.value))
    }
}

pub type ParameterItemId = crate::ParameterItem<
    BuildId<crate::kind::ParameterWithDefault>,
    BuildId<crate::kind::Parameter>,
>;
type ParameterItemView<'a> =
    crate::ParameterItem<crate::ParameterWithDefault<'a>, crate::Parameter<'a>>;

/// The first half of the interval holds child IDs, so generic source-order traversal
/// can borrow a contiguous slice. Each matching word in the second half holds its role.
#[derive(Clone, Copy)]
pub struct ParameterItems<'a>(pub(super) NodeList<'a, crate::kind::Any>);

impl PartialEq for ParameterItems<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.roles() == other.roles() && self.0 == other.0
    }
}

impl<'a> ParameterItems<'a> {
    pub(super) const POSITIONAL_ONLY: u32 = crate::ParameterRole::PositionalOnly as u32;
    pub(super) const POSITIONAL_OR_KEYWORD: u32 = crate::ParameterRole::PositionalOrKeyword as u32;
    pub(super) const VARARG: u32 = crate::ParameterRole::Vararg as u32;
    pub(super) const KEYWORD_ONLY: u32 = crate::ParameterRole::KeywordOnly as u32;
    pub(super) const KWARG: u32 = crate::ParameterRole::Kwarg as u32;

    fn roles(self) -> &'a [u32] {
        let start = self.0.range.indices().end;
        &self.0.ast.extra[start..start + self.len()]
    }

    pub(super) fn parameter_roles(
        self,
    ) -> impl DoubleEndedIterator<Item = crate::ParameterRole>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        self.roles().iter().map(|role| match *role {
            Self::POSITIONAL_ONLY => crate::ParameterRole::PositionalOnly,
            Self::POSITIONAL_OR_KEYWORD => crate::ParameterRole::PositionalOrKeyword,
            Self::VARARG => crate::ParameterRole::Vararg,
            Self::KEYWORD_ONLY => crate::ParameterRole::KeywordOnly,
            Self::KWARG => crate::ParameterRole::Kwarg,
            _ => unreachable!("invalid parameter role"),
        })
    }

    pub(super) fn is_in_binding_order(self) -> bool {
        self.roles()
            .iter()
            .copied()
            .is_sorted_by(|a, b| a < b || (a == b && !matches!(*a, Self::VARARG | Self::KWARG)))
    }

    /// Check the role before looking up the child row.
    pub(super) fn matching<N: AstNode<'a>>(
        self,
        accepts: impl Fn(u32) -> bool + Clone,
    ) -> impl DoubleEndedIterator<Item = N> + Clone {
        self.0
            .words()
            .iter()
            .zip(self.roles())
            .filter(move |(_, role)| accepts(**role))
            .map(move |(&id, _)| self.0.ast.child(u64::from(id)))
    }

    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = ParameterItemView<'a>>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        self.0
            .iter()
            .zip(self.roles())
            .map(|(child, &role)| match role {
                Self::POSITIONAL_ONLY => crate::ParameterItem::PositionalOnly(child.retype()),
                Self::POSITIONAL_OR_KEYWORD => {
                    crate::ParameterItem::PositionalOrKeyword(child.retype())
                }
                Self::VARARG => crate::ParameterItem::Vararg(child.retype()),
                Self::KEYWORD_ONLY => crate::ParameterItem::KeywordOnly(child.retype()),
                Self::KWARG => crate::ParameterItem::Kwarg(child.retype()),
                _ => unreachable!("invalid parameter role"),
            })
    }

    pub(crate) fn len(self) -> usize {
        self.0.len()
    }
    pub(super) fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn copy_to<R: super::Rewriter + ?Sized>(
        self,
        builder: &mut Builder,
        rewriter: &mut R,
    ) -> ParameterItemsId {
        let children =
            builder.map_list(self.0, |builder, child| builder.copy_with(child, rewriter));
        builder.ast.extra.extend_from_slice(self.roles());
        ParameterItemsId(children)
    }
}

impl std::fmt::Debug for ParameterItems<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ParameterItemsId(ListId<AstNodeId<crate::kind::Any>>);
impl ParameterItemsId {
    pub(super) fn word(self) -> u64 {
        self.0.word()
    }
}

impl ParameterItemId {
    fn words(self) -> (u32, u32) {
        match self {
            Self::PositionalOnly(p) => (p.slot, ParameterItems::POSITIONAL_ONLY),
            Self::PositionalOrKeyword(p) => (p.slot, ParameterItems::POSITIONAL_OR_KEYWORD),
            Self::Vararg(p) => (p.slot, ParameterItems::VARARG),
            Self::KeywordOnly(p) => (p.slot, ParameterItems::KEYWORD_ONLY),
            Self::Kwarg(p) => (p.slot, ParameterItems::KWARG),
        }
    }
}

impl Builder {
    pub fn parameter_items(
        &mut self,
        items: impl IntoIterator<Item = ParameterItemId>,
    ) -> ParameterItemsId {
        let start = self.ast.extra.len();
        let scratch = self.scratch.len();
        for item in items {
            let (id, role) = item.words();
            self.ast.extra.push(id);
            self.scratch.push(role);
        }
        let len = self.ast.extra.len() - start;
        self.ast.extra.extend_from_slice(&self.scratch[scratch..]);
        self.scratch.truncate(scratch);
        ParameterItemsId(ListId::new(Interval::new(start, len)))
    }
}

pub type MappingPatternItemId = MappingPatternItem<
    BuildId<crate::kind::Expr>,
    BuildId<crate::kind::Pattern>,
    BuildId<crate::kind::Identifier>,
>;
type MappingPatternItemView<'a> = MappingPatternItem<Expr<'a>, Pattern<'a>, Identifier<'a>>;

/// Source-ordered handles for a mapping pattern. Each key is immediately followed by its
/// pattern. A `**name` contributes its identifier alone, including in recovered syntax.
#[derive(Clone, Copy, PartialEq)]
pub struct MappingPatternItems<'a>(pub(super) NodeList<'a, crate::kind::Any>);

impl<'a> MappingPatternItems<'a> {
    /// Each entry has two children and each `**name` has one.
    pub(crate) fn child_count(self) -> usize {
        self.0.len()
    }

    pub fn keys(self) -> impl Iterator<Item = Expr<'a>> {
        self.entries().map(|(key, _)| key)
    }

    pub(super) fn patterns(self) -> impl Iterator<Item = Pattern<'a>> {
        self.entries().map(|(_, pattern)| pattern)
    }

    fn entries(self) -> impl Iterator<Item = (Expr<'a>, Pattern<'a>)> {
        self.into_iter().filter_map(|item| match item {
            MappingPatternItem::Entry { key, pattern } => Some((key, pattern)),
            MappingPatternItem::Rest(_) => None,
        })
    }

    pub fn rest(self) -> Option<Identifier<'a>> {
        self.0.iter().rev().find_map(AstNode::cast)
    }

    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn copy_to<R: super::Rewriter + ?Sized>(
        self,
        builder: &mut Builder,
        rewriter: &mut R,
    ) -> MappingPatternItemsId {
        MappingPatternItemsId(
            builder.map_list(self.0, |builder, child| builder.copy_with(child, rewriter)),
        )
    }
}

impl std::fmt::Debug for MappingPatternItems<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(*self).finish()
    }
}

impl<'a> IntoIterator for MappingPatternItems<'a> {
    type Item = MappingPatternItemView<'a>;
    type IntoIter = MappingPatternIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        MappingPatternIter(self.0.into_iter())
    }
}

/// The entries and `**name` captures of a mapping pattern in source order.
pub struct MappingPatternIter<'a>(Iter<'a, AstNodeId<crate::kind::Any>>);

impl<'a> Iterator for MappingPatternIter<'a> {
    type Item = MappingPatternItemView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let first = self.0.next()?;
        Some(if let Some(rest) = first.cast::<Identifier>() {
            MappingPatternItem::Rest(rest)
        } else {
            MappingPatternItem::Entry {
                key: first.retype(),
                pattern: self.0.next()?.retype(),
            }
        })
    }
}

/// A list of mapping entries and captures written by the same builder.
#[derive(Clone, Copy, Debug, Default)]
pub struct MappingPatternItemsId(ListId<AstNodeId<crate::kind::Any>>);

impl MappingPatternItemsId {
    pub(super) fn word(self) -> u64 {
        self.0.word()
    }
}

impl Builder {
    pub fn mapping_pattern_items(
        &mut self,
        items: impl IntoIterator<Item = MappingPatternItemId>,
    ) -> MappingPatternItemsId {
        let children = items.into_iter().flat_map(|item| {
            let (first, second) = match item {
                MappingPatternItem::Entry { key, pattern } => {
                    (key.cast::<Any>(), Some(pattern.cast()))
                }
                MappingPatternItem::Rest(identifier) => (identifier.cast(), None),
            };
            std::iter::once(first).chain(second)
        });
        MappingPatternItemsId(self.list(children))
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FStringPartView<'a> {
    Literal(StringLiteral<'a>),
    FString(FString<'a>),
}

impl Element for crate::FStringPart {
    type Item<'a> = FStringPartView<'a>;

    fn read(word: ListWord<'_, Self>) -> Self::Item<'_> {
        let node = word.node();
        if let Some(fstring) = node.cast::<FString>() {
            FStringPartView::FString(fstring)
        } else {
            FStringPartView::Literal(node.retype())
        }
    }
}

/// A borrowed list whose stored descriptor is an eight-byte start and length.
pub struct List<'a, E> {
    ast: &'a Ast,
    range: Interval,
    element: PhantomData<fn() -> E>,
}

impl<E> Copy for List<'_, E> {}
impl<E> Clone for List<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: Element> Default for List<'_, E> {
    fn default() -> Self {
        static EMPTY: Ast = Ast::empty();
        Self::new(&EMPTY, 0)
    }
}
impl<'a, E: Element> std::fmt::Debug for List<'a, E>
where
    E::Item<'a>: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(*self).finish()
    }
}

impl<'a, E: Element> List<'a, E> {
    pub(super) fn ast(self) -> &'a Ast {
        self.ast
    }

    pub(super) fn new(ast: &'a Ast, word: u64) -> Self {
        if E::IS_NODE {
            ast.validate_list(word);
        }
        // SAFETY: validate_list checked the interval and all node slots against
        // this AST. Scalar lists use checked indexing in words().
        #[expect(unsafe_code)]
        unsafe {
            Self::from_validated(ast, word)
        }
    }

    /// # Safety
    ///
    /// If `E::IS_NODE`, the interval must be in bounds for `ast.extra`, and
    /// every word in the interval must be an in-bounds slot in `ast.nodes`.
    #[expect(unsafe_code)]
    pub(super) unsafe fn from_validated(ast: &'a Ast, word: u64) -> Self {
        Self {
            ast,
            range: Interval::decode(word),
            element: PhantomData,
        }
    }

    pub fn len(self) -> usize {
        self.range.len as usize
    }

    pub fn id(self) -> ListId<E> {
        ListId::new(self.range)
    }

    pub fn is_empty(self) -> bool {
        self.range.len == 0
    }

    #[must_use]
    pub fn slice(self, range: impl std::ops::RangeBounds<usize>) -> Self {
        self.get_range(range).expect("list slice out of bounds")
    }

    pub fn get_range(self, range: impl std::ops::RangeBounds<usize>) -> Option<Self> {
        let start = match range.start_bound() {
            Bound::Included(&index) => index,
            Bound::Excluded(&index) => index.checked_add(1)?,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&index) => index.checked_add(1)?,
            Bound::Excluded(&index) => index,
            Bound::Unbounded => self.len(),
        };
        (start <= end && end <= self.len()).then(|| Self {
            range: Interval::new(self.range.start as usize + start, end - start),
            ..self
        })
    }

    pub fn get(self, index: usize) -> Option<E::Item<'a>> {
        (index < self.len()).then(|| self.at(index))
    }

    #[inline]
    pub fn at(self, index: usize) -> E::Item<'a> {
        assert!(index < self.len(), "list index out of bounds");
        E::read(ListWord {
            ast: self.ast,
            value: self.words()[index],
            element: PhantomData,
        })
    }

    pub fn as_array<const N: usize>(self) -> Option<[E::Item<'a>; N]> {
        (self.len() == N).then(|| std::array::from_fn(|i| self.at(i)))
    }

    pub fn split_at(self, index: usize) -> (Self, Self) {
        (self.slice(..index), self.slice(index..))
    }

    pub(super) fn words(self) -> &'a [u32] {
        let start = self.range.start as usize;
        if E::IS_NODE {
            // SAFETY: The interval belongs to this AST by from_validated's contract.
            // Slicing and iterator cursors only select subranges of that interval.
            #[expect(unsafe_code)]
            unsafe {
                self.ast.extra.get_unchecked(start..start + self.len())
            }
        } else {
            &self.ast.extra[start..start + self.len()]
        }
    }

    pub fn split_first(self) -> Option<(E::Item<'a>, Self)> {
        Some((self.first()?, self.slice(1..self.len())))
    }

    pub fn split_first_chunk<const N: usize>(self) -> Option<([E::Item<'a>; N], Self)> {
        let rest = self.get_range(N..)?;
        Some((std::array::from_fn(|i| self.at(i)), rest))
    }

    pub fn split_last_chunk<const N: usize>(self) -> Option<(Self, [E::Item<'a>; N])> {
        let start = self.len().checked_sub(N)?;
        Some((
            self.slice(..start),
            std::array::from_fn(|i| self.at(start + i)),
        ))
    }

    pub fn split_last(self) -> Option<(E::Item<'a>, Self)> {
        Some((self.last()?, self.slice(0..self.len() - 1)))
    }

    pub fn first(self) -> Option<E::Item<'a>> {
        self.get(0)
    }

    pub fn last(self) -> Option<E::Item<'a>> {
        self.get(self.len().checked_sub(1)?)
    }

    pub fn partition_point(self, mut predicate: impl FnMut(E::Item<'a>) -> bool) -> usize {
        let mut start = 0;
        let mut end = self.len();
        while start < end {
            let middle = start + (end - start) / 2;
            if predicate(self.at(middle)) {
                start = middle + 1;
            } else {
                end = middle;
            }
        }
        start
    }

    // This is only for lists whose stored words are node IDs.
    pub(super) fn position_of_node(self, node: super::SyntaxNode<'a>) -> Option<usize> {
        if !std::ptr::eq(self.ast, node.ast()) {
            return None;
        }
        let slot = node.node_id().slot.get();
        self.words().iter().position(|&word| word == slot)
    }

    #[inline]
    pub fn iter(self) -> Iter<'a, E> {
        self.into_iter()
    }
}

pub struct Iter<'a, E> {
    ast: &'a Ast,
    words: &'a [u32],
    marker: PhantomData<E>,
}

impl<E> Clone for Iter<'_, E> {
    fn clone(&self) -> Self {
        Self { ..*self }
    }
}

impl<'a, E: Element> Iter<'a, E> {
    #[inline]
    fn decode(&self, value: u32) -> E::Item<'a> {
        E::read(ListWord {
            ast: self.ast,
            value,
            element: PhantomData,
        })
    }

    pub fn next_if(&mut self, predicate: impl FnOnce(E::Item<'a>) -> bool) -> Option<E::Item<'a>> {
        let mut probe = self.clone();
        let item = probe.next()?;
        if predicate(item) {
            *self = probe;
            Some(item)
        } else {
            None
        }
    }

    pub fn as_slice(&self) -> List<'a, E> {
        let words = self.words;
        List {
            ast: self.ast,
            // `words` always remains a subslice of this arena, including when exhausted.
            range: Interval::new(
                (words.as_ptr().addr() - self.ast.extra.as_ptr().addr()) / size_of::<u32>(),
                self.len(),
            ),
            element: PhantomData,
        }
    }
}

impl<'a, E: Element> IntoIterator for List<'a, E> {
    type Item = E::Item<'a>;
    type IntoIter = Iter<'a, E>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let words = self.words();
        Iter {
            ast: self.ast,
            words,
            marker: PhantomData,
        }
    }
}

impl<'a, E: Element> Iterator for Iter<'a, E> {
    type Item = E::Item<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = *self.words.split_off_first()?;
        Some(self.decode(value))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        // Skip encoded words before decoding the selected item.
        self.words = &self.words[n.min(self.words.len())..];
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.words.len();
        (count, Some(count))
    }
}

impl<E: Element> DoubleEndedIterator for Iter<'_, E> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let value = *self.words.split_off_last()?;
        Some(self.decode(value))
    }
}

impl<E: Element> ExactSizeIterator for Iter<'_, E> {}
impl<E: Element> std::iter::FusedIterator for Iter<'_, E> {}

impl<'a, T: NodeType + 'static> NodeList<'a, T> {
    /// Finds the first occurrence of this node by its stored arena ID.
    pub fn position_of(self, node: impl AstNode<'a>) -> Option<usize> {
        self.position_of_node(node.syntax_node())
    }

    pub(super) fn starts_with_node(self, node: super::SyntaxNode<'a>) -> bool {
        self.first().is_some_and(|first| first.is_same_node(node))
    }
}

impl<'a> List<'a, AstNodeId<crate::kind::BytesLiteral>> {
    pub fn bytes(self) -> impl DoubleEndedIterator<Item = u8> + 'a {
        self.iter().flat_map(|part| part.value().iter().copied())
    }
}

impl<'a, E: Element> PartialEq for List<'a, E>
where
    E::Item<'a>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextRange;

    use crate::AstNode;
    use crate::flat::{Builder, Emit, build};
    use crate::kind;

    #[test]
    fn builder_checks_the_selected_list_item() {
        let range = TextRange::default();
        let mut builder = Builder::default();
        let first = builder.add(build::StmtPass { range });
        let checkpoint = builder.checkpoint();
        let stale = builder.add(build::StmtPass { range });
        builder.rewind(checkpoint);
        let list = builder.list([first, stale]);
        assert!(builder.list_item(list, 0).is_same_node(builder.get(first)));
        assert!(std::panic::catch_unwind(|| builder.list_item(list, 1)).is_err());
    }

    #[test]
    fn node_positions_use_ids_from_the_same_tree() {
        let range = TextRange::default();
        let mut builder = Builder::default();
        let a = build::StmtPass { range }.emit(&mut builder);
        let b = build::StmtPass { range }.emit(&mut builder);
        let ids = builder.list([a, b, a]);
        let list = builder.get_list(ids);
        assert_eq!(list.position_of(builder.get(a)), Some(0));
        assert_eq!(list.position_of(builder.get(b)), Some(1));

        let mut other = Builder::default();
        let a = build::StmtPass { range }.emit(&mut other);
        assert_eq!(list.position_of(other.get(a)), None);
    }

    #[test]
    fn filtered_nodes_track_mixed_argument_positions() {
        let range = TextRange::default();
        let mut builder = Builder::default();
        let a = build::ExprNoneLiteral { range }.emit(&mut builder);
        let b = build::ExprNoneLiteral { range }.emit(&mut builder);
        let a: crate::flat::BuildId<kind::Expr> = a.into();
        let b: crate::flat::BuildId<kind::Expr> = b.into();
        let keyword_a = build::Keyword {
            range,
            arg: None,
            value: a,
        }
        .emit(&mut builder);
        let keyword_b = build::Keyword {
            range,
            arg: None,
            value: b,
        }
        .emit(&mut builder);
        let ids = builder.list([
            super::ArgumentId::Positional(a),
            super::ArgumentId::Keyword(keyword_a),
            super::ArgumentId::Positional(b),
            super::ArgumentId::Keyword(keyword_b),
        ]);
        let list = builder.get_list(ids);
        let args = super::FilteredNodes::<kind::Expr>::new(list);
        let keywords = super::FilteredNodes::<kind::Keyword>::new(list);

        assert_eq!(
            args.skip_front(0).as_array(),
            Some([builder.get(a), builder.get(b)])
        );
        assert_eq!(
            args.split_first()
                .map(|(first, rest)| (first, rest.as_array())),
            Some((builder.get(a), Some([builder.get(b)]))),
        );
        assert_eq!(args.skip_front(1).first(), Some(builder.get(b)));
        assert!(args.skip_front(2).is_empty());
        assert!(args.skip_front(usize::MAX).is_empty());
        assert_eq!(args.as_array::<2>(), Some([builder.get(a), builder.get(b)]));
        assert!(args.as_array::<3>().is_none());
        assert!(args.as_array::<5>().is_none());
        assert_eq!(
            keywords.as_array(),
            Some([builder.get(keyword_a), builder.get(keyword_b)])
        );
        assert_eq!(
            keywords
                .split_first()
                .map(|(first, rest)| (first, rest.as_array())),
            Some((builder.get(keyword_a), Some([builder.get(keyword_b)]))),
        );
        assert!(keywords.skip_front(1).skip_front(1).is_empty());
        let empty = super::FilteredNodes::<kind::Keyword>::new(list.slice(..1));
        assert_eq!(empty.as_array::<0>(), Some([]));
    }

    #[test]
    fn iterator_slice_tracks_front_and_back() {
        let range = TextRange::default();
        let mut builder = Builder::default();
        let a = build::StmtPass { range }.emit(&mut builder);
        let b = build::StmtPass { range }.emit(&mut builder);
        let c = build::StmtPass { range }.emit(&mut builder);
        let ids = builder.list([a, b, c]);
        let list = builder.get_list(ids);
        let mut iter = list.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.as_slice(), list);
        assert_eq!(iter.next(), list.get(0));
        assert_eq!(iter.next_back(), list.get(2));
        assert_eq!(iter.as_slice(), list.slice(1..2));
        assert_eq!(iter.as_slice().range.start, list.range.start + 1);
        assert_eq!(iter.as_slice().range.len, 1);
        let clone = iter.clone();
        assert_eq!(clone.as_slice(), list.slice(1..2));
        assert!(iter.next_if(|_| false).is_none());
        assert_eq!(iter.as_slice(), list.slice(1..2));
        assert_eq!(iter.next_if(|_| true), list.get(1));
        assert!(iter.as_slice().is_empty());
        assert!(iter.next().is_none());
        assert!(iter.next_back().is_none());
        let mut exhausted = list.iter();
        assert!(exhausted.nth(usize::MAX).is_none());
        assert_eq!(exhausted.as_slice(), list.slice(3..3));
        assert_eq!(exhausted.as_slice().range.start, list.range.start + 3);
        assert_eq!(exhausted.as_slice().range.len, 0);
        assert!(list.slice(0..0).iter().as_slice().is_empty());
    }
}
