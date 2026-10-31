use std::marker::PhantomData;
use std::ops::Bound;
use std::ops::Range;

use super::{Ast, BuildId, Handle, Node, View, offset, split_words};
use crate::{CmpOp, Expr, FString, NodeKind, StringLiteral};

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
    #[doc(hidden)]
    const WORDS: usize = 1;
    type Item<'a>: Copy;

    #[doc(hidden)]
    fn read(ast: &Ast, word: u64) -> Self::Item<'_>;
}

pub(super) fn store<E: Element>(dst: &mut Vec<u32>, words: impl Iterator<Item = u64>) {
    if E::WORDS == 2 {
        dst.extend(words.flat_map(split_words));
    } else {
        dst.extend(words.map(|word| split_words(word)[0]));
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

pub trait ListItem {
    type Element: Element;

    #[doc(hidden)]
    fn word(self) -> u64;
}

impl<T: Node + 'static> ListItem for Handle<T> {
    type Element = Self;

    fn word(self) -> u64 {
        u64::from(self.slot.get())
    }
}

impl<T: Node + 'static> ListItem for BuildId<T> {
    type Element = Handle<T>;

    fn word(self) -> u64 {
        u64::from(self.slot)
    }
}

impl ListItem for CmpOp {
    type Element = Self;

    fn word(self) -> u64 {
        super::scalar::Word::encode(self)
    }
}

impl ListItem for (Option<Handle<Expr>>, Handle<Expr>) {
    type Element = crate::DictItem;

    fn word(self) -> u64 {
        u64::from(self.0.map_or(0, |key| key.slot.get())) | (u64::from(self.1.slot.get()) << 32)
    }
}

impl ListItem for (Option<BuildId<Expr>>, BuildId<Expr>) {
    type Element = crate::DictItem;

    fn word(self) -> u64 {
        u64::from(self.0.map_or(0, |key| key.slot)) | (u64::from(self.1.slot) << 32)
    }
}

pub enum FStringPartId {
    Literal(BuildId<StringLiteral>),
    FString(BuildId<FString>),
}

impl ListItem for FStringPartId {
    type Element = crate::FStringPart;

    fn word(self) -> u64 {
        match self {
            Self::Literal(value) => u64::from(value.slot),
            Self::FString(value) => u64::from(value.slot),
        }
    }
}

impl<T: Node + 'static> Element for Handle<T> {
    type Item<'a> = View<'a, T>;

    fn read(ast: &Ast, word: u64) -> Self::Item<'_> {
        ast.child(word)
    }
}

impl Element for CmpOp {
    type Item<'a> = Self;

    fn read(_: &Ast, word: u64) -> Self {
        super::scalar::Word::decode(word)
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct DictItemView<'a> {
    pub key: Option<View<'a, Expr>>,
    pub value: View<'a, Expr>,
}

impl ruff_text_size::Ranged for DictItemView<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        ruff_text_size::TextRange::new(
            self.key
                .map_or_else(|| self.value.start(), |key| key.start()),
            self.value.end(),
        )
    }
}

impl Element for crate::DictItem {
    const WORDS: usize = 2;
    type Item<'a> = DictItemView<'a>;

    fn read(ast: &Ast, word: u64) -> Self::Item<'_> {
        DictItemView {
            key: ast.optional_child(word),
            value: ast.child(word >> 32),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FStringPartView<'a> {
    Literal(View<'a, StringLiteral>),
    FString(View<'a, FString>),
}

impl Element for crate::FStringPart {
    type Item<'a> = FStringPartView<'a>;

    fn read(ast: &Ast, word: u64) -> Self::Item<'_> {
        if ast.rows[split_words(word)[0] as usize].kind() == NodeKind::FString {
            FStringPartView::FString(ast.child(word))
        } else {
            FStringPartView::Literal(ast.child(word))
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
            range: Interval::new(self.range.start as usize + start * E::WORDS, end - start),
            ..self
        })
    }

    pub fn get(self, index: usize) -> Option<E::Item<'a>> {
        (index < self.len()).then(|| self.at(index))
    }

    #[inline]
    pub fn at(self, index: usize) -> E::Item<'a> {
        assert!(index < self.len(), "list index out of bounds");
        let slot = self.range.start as usize + index * E::WORDS;
        let word = if E::WORDS == 2 {
            self.ast.extra_word::<true>(slot)
        } else {
            self.ast.extra_word::<false>(slot)
        };
        E::read(self.ast, word)
    }

    pub fn as_array<const N: usize>(self) -> Option<[E::Item<'a>; N]> {
        (self.len() == N).then(|| std::array::from_fn(|i| self.at(i)))
    }

    pub fn split_at(self, index: usize) -> (Self, Self) {
        (self.slice(..index), self.slice(index..))
    }

    fn words(self) -> &'a [u32] {
        let start = self.range.start as usize;
        &self.ast.extra[start..start + self.len() * E::WORDS]
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

impl<T: Node> List<'_, Handle<T>> {
    /// Visit the first child directly and leave its siblings for later.
    #[inline]
    pub(super) fn first_descendant(
        self,
        stack: &mut Vec<Handle<super::Any>>,
    ) -> Option<Handle<super::Any>> {
        let Some((&first, rest)) = self.words().split_first() else {
            return stack.pop();
        };
        if !rest.is_empty() {
            stack.extend(rest.iter().rev().map(|&row| Handle::from_word(row)));
        }
        Some(Handle::from_word(first))
    }

    #[inline]
    pub(super) fn push_reversed(self, stack: &mut Vec<Handle<super::Any>>) {
        stack.extend(self.words().iter().rev().map(|&row| Handle::from_word(row)));
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
    #[inline]
    fn read(&self, words: &[u32]) -> E::Item<'a> {
        let word = u64::from(words[0]);
        let word = if E::WORDS == 2 {
            word | (u64::from(words[1]) << 32)
        } else {
            word
        };
        E::read(self.ast, word)
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
        let (item, rest) = self.words.split_at_checked(E::WORDS)?;
        self.words = rest;
        Some(self.read(item))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.words = self
            .words
            .get(n.saturating_mul(E::WORDS)..)
            .unwrap_or_default();
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.words.len() / E::WORDS;
        (count, Some(count))
    }
}

impl<E: Element> DoubleEndedIterator for Iter<'_, E> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let start = self.words.len().checked_sub(E::WORDS)?;
        let (rest, item) = self.words.split_at(start);
        self.words = rest;
        self.end -= E::WORDS;
        Some(self.read(item))
    }
}

impl<E: Element> ExactSizeIterator for Iter<'_, E> {}
impl<E: Element> std::iter::FusedIterator for Iter<'_, E> {}

impl DictItemView<'_> {
    pub(super) fn to_owned_item(self) -> crate::DictItem {
        crate::DictItem {
            key: self.key.map(View::to_owned_node),
            value: self.value.to_owned_node(),
        }
    }
}

macro_rules! literal_parts {
    ($part:ty, $value:ty) => {
        impl List<'_, Handle<$part>> {
            pub(super) fn to_literal(self) -> $value {
                if self.len() == 1
                    && let Some(part) = self.first()
                {
                    <$value>::single(part.to_owned_node())
                } else {
                    <$value>::concatenated(self.iter().map(View::to_owned_node).collect())
                }
            }
        }
    };
}

literal_parts!(crate::StringLiteral, crate::StringLiteralValue);
literal_parts!(crate::TString, crate::TStringValue);
literal_parts!(crate::BytesLiteral, crate::BytesLiteralValue);

impl List<'_, crate::FStringPart> {
    pub(super) fn to_literal(self) -> crate::FStringValue {
        if self.len() == 1
            && let Some(FStringPartView::FString(part)) = self.first()
        {
            crate::FStringValue::single(part.to_owned_node())
        } else {
            crate::FStringValue::concatenated(
                self.iter()
                    .map(|part| match part {
                        FStringPartView::FString(value) => {
                            crate::FStringPart::FString(value.to_owned_node())
                        }
                        FStringPartView::Literal(value) => {
                            crate::FStringPart::Literal(value.to_owned_node())
                        }
                    })
                    .collect(),
            )
        }
    }
}

impl<'a> List<'a, Handle<crate::BytesLiteral>> {
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

impl std::fmt::Debug for DictItemView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DictItem")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
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
