use std::marker::PhantomData;
use std::ops::Bound;
use std::ops::Range;

use super::{
    Any, Ast, AstNode, AstNodeId, BuildId, Builder, NodeList, NodeType, offset, split_words,
};
use crate::{
    CmpOp, Expr, FString, Identifier, MappingPatternItem, NodeKind, Pattern, StringLiteral,
};

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

    /// Store two lists of single-word handles with a shared boundary. The parser
    /// appends the two lists consecutively. Other builders can supply existing
    /// lists from anywhere in the extra-data column.
    pub(super) fn pack_pair(extra: &mut Vec<u32>, pair: [u64; 2]) -> [u64; 2] {
        let [first, second] = pair.map(Self::decode);
        let boundary = if first.len == 0 {
            second.start
        } else if second.len == 0 || first.indices().end == second.start as usize {
            offset(first.indices().end)
        } else {
            extra.extend_from_within(first.indices());
            let boundary = offset(extra.len());
            extra.extend_from_within(second.indices());
            boundary
        };
        let second = Self {
            start: boundary,
            len: second.len,
        };
        [u64::from(first.len), second.encode()]
    }

    pub(super) fn preceding(boundary: u64, len: u64) -> u64 {
        let len = split_words(len)[0];
        Self {
            start: split_words(boundary)[0] - len,
            len,
        }
        .encode()
    }

    pub(super) fn indices(self) -> Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

const _: () = assert!(size_of::<Interval>() == 8);

/// Items stored in an AST list. The field schema determines how each word is read.
pub trait Element {
    type Item<'a>: Copy;

    #[doc(hidden)]
    fn read(ast: &Ast, word: u32) -> Self::Item<'_>;
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

    fn read(ast: &Ast, word: u32) -> Self::Item<'_> {
        let node: super::SyntaxNode<'_> = ast.child(u64::from(word));
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

    fn read(ast: &Ast, word: u32) -> Self::Item<'_> {
        let node: super::SyntaxNode<'_> = ast.child(u64::from(word));
        if let Some(keyword) = node.cast::<crate::PatternKeyword>() {
            crate::PatternOrKeyword::Keyword(keyword)
        } else {
            crate::PatternOrKeyword::Pattern(node.retype())
        }
    }
}

/// The children of one type in a heterogeneous, source-ordered AST list.
pub struct FilteredNodes<'a, T> {
    ast: &'a Ast,
    words: &'a [u32],
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
        Self {
            ast: items.ast,
            words: items.words(),
            marker: PhantomData,
        }
    }

    pub fn iter(self) -> FilteredNodeIter<'a, T> {
        FilteredNodeIter(self)
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

    #[track_caller]
    pub fn at(self, index: usize) -> T::Borrowed<'a> {
        self.get(index).expect("AST child index out of bounds")
    }

    pub fn split_first(self) -> Option<(T::Borrowed<'a>, Self)> {
        let mut iter = self.iter();
        Some((iter.next()?, iter.0))
    }

    #[must_use]
    pub fn skip_front(self, n: usize) -> Self {
        let mut iter = self.iter();
        if n > 0 {
            iter.nth(n - 1);
        }
        iter.0
    }

    pub fn first(self) -> Option<T::Borrowed<'a>> {
        self.iter().next()
    }
    pub fn last(self) -> Option<T::Borrowed<'a>> {
        self.iter().next_back()
    }

    pub fn as_array<const N: usize>(self) -> Option<[T::Borrowed<'a>; N]> {
        itertools::Itertools::collect_array(self.iter())
    }
}

impl<'a, T: NodeType> IntoIterator for FilteredNodes<'a, T> {
    type Item = T::Borrowed<'a>;
    type IntoIter = FilteredNodeIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct FilteredNodeIter<'a, T>(FilteredNodes<'a, T>);

impl<T> Clone for FilteredNodeIter<'_, T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<'a, T: NodeType> Iterator for FilteredNodeIter<'a, T> {
    type Item = T::Borrowed<'a>;
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.0.words.len()))
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(word) = self.0.words.split_off_first() {
            let node: super::SyntaxNode<'_> = self.0.ast.child(u64::from(*word));
            if let Some(node) = node.cast::<T::Borrowed<'a>>() {
                return Some(node);
            }
        }
        None
    }
}

impl<'a, T: NodeType> From<NodeList<'a, T>> for FilteredNodes<'a, T> {
    fn from(nodes: NodeList<'a, T>) -> Self {
        Self::new(nodes)
    }
}

impl<'a, T: NodeType> DoubleEndedIterator for FilteredNodeIter<'a, T> {
    fn next_back(&mut self) -> Option<T::Borrowed<'a>> {
        while let Some(word) = self.0.words.split_off_last() {
            let node: super::SyntaxNode<'_> = self.0.ast.child(u64::from(*word));
            if let Some(node) = node.cast::<T::Borrowed<'a>>() {
                return Some(node);
            }
        }
        None
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

    fn read(ast: &Ast, word: u32) -> Self::Item<'_> {
        ast.child(u64::from(word))
    }
}

impl Element for CmpOp {
    type Item<'a> = Self;

    fn read(_: &Ast, word: u32) -> Self {
        super::scalar::Word::decode(u64::from(word))
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

    pub fn len(self) -> usize {
        self.0.len()
    }
    pub fn is_empty(self) -> bool {
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
    pub fn entries(self) -> impl Iterator<Item = (Expr<'a>, Pattern<'a>)> {
        self.into_iter().filter_map(|item| match item {
            MappingPatternItem::Entry { key, pattern } => Some((key, pattern)),
            MappingPatternItem::Rest(_) => None,
        })
    }

    pub fn rest(self) -> Option<Identifier<'a>> {
        self.0.iter().rev().find_map(AstNode::cast::<Identifier>)
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

    fn read(ast: &Ast, word: u32) -> Self::Item<'_> {
        if ast.rows[word as usize].kind() == NodeKind::FString {
            FStringPartView::FString(ast.child(u64::from(word)))
        } else {
            FStringPartView::Literal(ast.child(u64::from(word)))
        }
    }
}

/// A borrowed list whose stored descriptor is an eight-byte start and length.
pub struct List<'a, E> {
    pub(super) ast: &'a Ast,
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
        static EMPTY: std::sync::LazyLock<Ast> = std::sync::LazyLock::new(Ast::default);
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
    pub(super) fn new(ast: &'a Ast, word: u64) -> Self {
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
        E::read(self.ast, self.ast.extra[self.range.start as usize + index])
    }

    pub fn as_array<const N: usize>(self) -> Option<[E::Item<'a>; N]> {
        (self.len() == N).then(|| std::array::from_fn(|i| self.at(i)))
    }

    pub fn split_at(self, index: usize) -> (Self, Self) {
        (self.slice(..index), self.slice(index..))
    }

    pub(super) fn words(self) -> &'a [u32] {
        let start = self.range.start as usize;
        &self.ast.extra[start..start + self.len()]
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

    pub fn iter(self) -> Iter<'a, E> {
        self.into_iter()
    }
}

pub struct Iter<'a, E> {
    ast: &'a Ast,
    words: &'a [u32],
    end: usize,
    marker: PhantomData<E>,
}

impl<E> Clone for Iter<'_, E> {
    fn clone(&self) -> Self {
        Self { ..*self }
    }
}

impl<'a, E: Element> Iter<'a, E> {
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
        List {
            ast: self.ast,
            range: Interval::new(self.end - self.words.len(), self.len()),
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
            end: self.range.start as usize + words.len(),
            marker: PhantomData,
        }
    }
}

impl<'a, E: Element> Iterator for Iter<'a, E> {
    type Item = E::Item<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(E::read(self.ast, *self.words.split_off_first()?))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.words = self.words.get(n..).unwrap_or_default();
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
        let word = *self.words.split_off_last()?;
        self.end -= 1;
        Some(E::read(self.ast, word))
    }
}

impl<E: Element> ExactSizeIterator for Iter<'_, E> {}
impl<E: Element> std::iter::FusedIterator for Iter<'_, E> {}

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
    use super::Interval;

    #[test]
    fn paired_lists_share_a_boundary() {
        for (first, second, copied) in [
            (0..2, 2..4, 0),
            (3..5, 0..2, 4),
            (2..2, 0..2, 0),
            (1..4, 0..0, 0),
            (2..2, 5..5, 0),
        ] {
            let original = [10, 20, 30, 40, 50];
            let mut extra = original.to_vec();
            let pair = [&first, &second].map(|r| Interval::new(r.start, r.len()).encode());
            let [len, second_word] = Interval::pack_pair(&mut extra, pair);
            let first_word = Interval::preceding(second_word, len);
            assert_eq!(extra.len(), original.len() + copied);
            assert_eq!(extra[..original.len()], original);
            assert_eq!(
                extra[Interval::decode(first_word).indices()],
                original[first]
            );
            assert_eq!(
                extra[Interval::decode(second_word).indices()],
                original[second]
            );
        }
    }
}
