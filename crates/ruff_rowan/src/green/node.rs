use std::convert::TryFrom;
use std::fmt::Formatter;
use std::iter::Enumerate;
use std::{
    borrow::{Borrow, Cow},
    fmt,
    iter::FusedIterator,
    mem::{self, ManuallyDrop},
    ops, ptr, slice,
};

#[cfg(target_pointer_width = "64")]
use crate::utility_types::static_assert;

use countme::Count;

use crate::{
    arc::{Arc, HeaderSlice, ThinArc},
    green::{GreenElement, GreenElementRef, RawSyntaxKind},
    GreenToken, NodeOrToken, TextRange, TextSize,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct GreenNodeHead {
    kind: RawSyntaxKind,
    text_len: TextSize,
    _c: Count<GreenNode>,
}

pub(crate) fn has_live() -> bool {
    countme::get::<GreenNode>().live > 0
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Slot {
    Node {
        rel_offset: TextSize,
        node: GreenNode,
    },
    Token {
        rel_offset: TextSize,
        token: GreenToken,
    },
    /// An empty slot for a child that was missing in the source because:
    /// * it's an optional child which is missing for this node
    /// * it's a mandatory child but it's missing because of a syntax error
    Empty { rel_offset: TextSize },
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Slot::Empty { .. } => write!(f, "∅"),
            Slot::Node { node, .. } => std::fmt::Display::fmt(node, f),
            Slot::Token { token, .. } => std::fmt::Display::fmt(token, f),
        }
    }
}

#[cfg(target_pointer_width = "64")]
static_assert!(mem::size_of::<Slot>() == mem::size_of::<usize>() * 2);

type Repr = HeaderSlice<GreenNodeHead, [Slot]>;
type ReprThin = HeaderSlice<GreenNodeHead, [Slot; 0]>;
#[repr(transparent)]
pub(crate) struct GreenNodeData {
    data: ReprThin,
}

impl PartialEq for GreenNodeData {
    fn eq(&self, other: &Self) -> bool {
        self.header() == other.header() && self.slice() == other.slice()
    }
}

/// Internal node in the immutable tree.
/// It has other nodes and tokens as children.
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(crate) struct GreenNode {
    ptr: ThinArc<GreenNodeHead, Slot>,
}

impl ToOwned for GreenNodeData {
    type Owned = GreenNode;

    #[inline]
    fn to_owned(&self) -> GreenNode {
        unsafe {
            let green = GreenNode::from_raw(ptr::NonNull::from(self));
            let green = ManuallyDrop::new(green);
            GreenNode::clone(&green)
        }
    }
}

impl Borrow<GreenNodeData> for GreenNode {
    #[inline]
    fn borrow(&self) -> &GreenNodeData {
        self
    }
}

impl From<Cow<'_, GreenNodeData>> for GreenNode {
    #[inline]
    fn from(cow: Cow<'_, GreenNodeData>) -> Self {
        cow.into_owned()
    }
}

impl From<&'_ GreenNodeData> for GreenNode {
    #[inline]
    fn from(borrow: &'_ GreenNodeData) -> Self {
        borrow.to_owned()
    }
}

impl fmt::Debug for GreenNodeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GreenNode")
            .field("kind", &self.kind())
            .field("text_len", &self.text_len())
            .field("n_slots", &self.slots().len())
            .finish()
    }
}

impl fmt::Debug for GreenNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data: &GreenNodeData = self;
        fmt::Debug::fmt(data, f)
    }
}

impl fmt::Display for GreenNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data: &GreenNodeData = self;
        fmt::Display::fmt(data, f)
    }
}

impl fmt::Display for GreenNodeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for child in self.slots() {
            write!(f, "{}", child)?;
        }
        Ok(())
    }
}

impl GreenNodeData {
    #[inline]
    fn header(&self) -> &GreenNodeHead {
        &self.data.header
    }

    #[inline]
    pub(crate) fn slice(&self) -> &[Slot] {
        self.data.slice()
    }

    /// Kind of this node.
    #[inline]
    pub fn kind(&self) -> RawSyntaxKind {
        self.header().kind
    }

    /// Returns the length of the text covered by this node.
    #[inline]
    pub fn text_len(&self) -> TextSize {
        self.header().text_len
    }

    /// Children of this node.
    #[inline]
    pub fn children(&self) -> Children<'_> {
        Children::new(self.slots().enumerate())
    }

    /// Returns the slots of this node. Every node of a specific kind has the same number of slots
    /// to allow using fixed offsets to retrieve a specific child even if some other child is missing.
    #[inline]
    pub fn slots(&self) -> Slots<'_> {
        Slots {
            raw: self.slice().iter(),
        }
    }

    pub(crate) fn slot_at_range(
        &self,
        rel_range: TextRange,
    ) -> Option<(usize, TextSize, &'_ Slot)> {
        let idx = self
            .slice()
            .binary_search_by(|it| {
                let child_range = it.rel_range();
                TextRange::ordering(child_range, rel_range)
            })
            // XXX: this handles empty ranges
            .unwrap_or_else(|it| it.saturating_sub(1));
        let slot = &self
            .slice()
            .get(idx)
            .filter(|it| it.rel_range().contains_range(rel_range))?;
        Some((idx, slot.rel_offset(), slot))
    }

    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    pub(crate) fn splice_slots<R, I>(&self, range: R, replace_with: I) -> GreenNode
    where
        R: ops::RangeBounds<usize>,
        I: Iterator<Item = Option<GreenElement>>,
    {
        let mut slots: Vec<_> = self
            .slots()
            .map(|slot| match slot {
                Slot::Empty { .. } => None,
                Slot::Node { node, .. } => Some(NodeOrToken::Node(node.to_owned())),
                Slot::Token { token, .. } => Some(NodeOrToken::Token(token.to_owned())),
            })
            .collect();

        slots.splice(range, replace_with);
        GreenNode::new(self.kind(), slots)
    }
}

impl ops::Deref for GreenNode {
    type Target = GreenNodeData;

    #[inline]
    fn deref(&self) -> &GreenNodeData {
        unsafe {
            let repr: &Repr = &self.ptr;
            let repr: &ReprThin = &*(repr as *const Repr as *const ReprThin);
            mem::transmute::<&ReprThin, &GreenNodeData>(repr)
        }
    }
}

impl GreenNode {
    /// Creates new Node.
    #[inline]
    pub fn new<I>(kind: RawSyntaxKind, slots: I) -> GreenNode
    where
        I: IntoIterator<Item = Option<GreenElement>>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut text_len: TextSize = 0.into();
        let slots = slots.into_iter().map(|el| {
            let rel_offset = text_len;
            match el {
                Some(el) => {
                    text_len += el.text_len();
                    match el {
                        NodeOrToken::Node(node) => Slot::Node { rel_offset, node },
                        NodeOrToken::Token(token) => Slot::Token { rel_offset, token },
                    }
                }
                None => Slot::Empty { rel_offset },
            }
        });

        let data = ThinArc::from_header_and_iter(
            GreenNodeHead {
                kind,
                text_len: 0.into(),
                _c: Count::new(),
            },
            slots,
        );

        // XXX: fixup `text_len` after construction, because we can't iterate
        // `slots` twice.
        let data = {
            let mut data = Arc::from_thin(data);
            Arc::get_mut(&mut data).unwrap().header.text_len = text_len;
            Arc::into_thin(data)
        };

        GreenNode { ptr: data }
    }

    #[inline]
    pub(crate) unsafe fn from_raw(ptr: ptr::NonNull<GreenNodeData>) -> GreenNode {
        let arc = Arc::from_raw(&ptr.as_ref().data as *const ReprThin);
        let arc = mem::transmute::<Arc<ReprThin>, ThinArc<GreenNodeHead, Slot>>(arc);
        GreenNode { ptr: arc }
    }
}

impl Slot {
    #[inline]
    pub(crate) fn as_ref(&self) -> Option<GreenElementRef> {
        match self {
            Slot::Node { node, .. } => Some(NodeOrToken::Node(node)),
            Slot::Token { token, .. } => Some(NodeOrToken::Token(token)),
            Slot::Empty { .. } => None,
        }
    }
    #[inline]
    pub(crate) fn rel_offset(&self) -> TextSize {
        match self {
            Slot::Node { rel_offset, .. }
            | Slot::Token { rel_offset, .. }
            | Slot::Empty { rel_offset } => *rel_offset,
        }
    }
    #[inline]
    fn rel_range(&self) -> TextRange {
        let text_len = match self.as_ref() {
            None => TextSize::from(0),
            Some(element) => element.text_len(),
        };

        TextRange::at(self.rel_offset(), text_len)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Slots<'a> {
    pub(crate) raw: slice::Iter<'a, Slot>,
}

// NB: forward everything stable that iter::Slice specializes as of Rust 1.39.0
impl ExactSizeIterator for Slots<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.raw.len()
    }
}

impl<'a> Iterator for Slots<'a> {
    type Item = &'a Slot;

    #[inline]
    fn next(&mut self) -> Option<&'a Slot> {
        self.raw.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.raw.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.raw.count()
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.next_back()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.raw.nth(n)
    }

    #[inline]
    fn fold<Acc, Fold>(self, init: Acc, mut f: Fold) -> Acc
    where
        Fold: FnMut(Acc, Self::Item) -> Acc,
    {
        let mut accum = init;
        for x in self {
            accum = f(accum, x);
        }
        accum
    }
}

impl<'a> DoubleEndedIterator for Slots<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.raw.next_back()
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.raw.nth_back(n)
    }

    #[inline]
    fn rfold<Acc, Fold>(mut self, init: Acc, mut f: Fold) -> Acc
    where
        Fold: FnMut(Acc, Self::Item) -> Acc,
    {
        let mut accum = init;
        while let Some(x) = self.next_back() {
            accum = f(accum, x);
        }
        accum
    }
}

impl FusedIterator for Slots<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct Child<'a> {
    slot: u32,
    rel_offset: TextSize,
    element: GreenElementRef<'a>,
}

impl<'a> Child<'a> {
    pub fn slot(&self) -> u32 {
        self.slot
    }
    pub fn rel_offset(&self) -> TextSize {
        self.rel_offset
    }
    pub fn element(&self) -> GreenElementRef<'a> {
        self.element
    }
}

impl<'a> TryFrom<(usize, &'a Slot)> for Child<'a> {
    type Error = ();

    fn try_from((index, slot): (usize, &'a Slot)) -> Result<Self, Self::Error> {
        match slot {
            Slot::Empty { .. } => Err(()),
            Slot::Node { node, rel_offset } => Ok(Child {
                element: NodeOrToken::Node(node),
                slot: index as u32,
                rel_offset: *rel_offset,
            }),
            Slot::Token { token, rel_offset } => Ok(Child {
                element: NodeOrToken::Token(token),
                slot: index as u32,
                rel_offset: *rel_offset,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Children<'a> {
    slots: Enumerate<Slots<'a>>,
}

impl<'a> Children<'a> {
    pub fn new(slots: Enumerate<Slots<'a>>) -> Self {
        Self { slots }
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = Child<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.slots.find_map(|it| Child::try_from(it).ok())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<'a> DoubleEndedIterator for Children<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.slots.next_back()?;

            if let Ok(child) = Child::try_from(next) {
                return Some(child);
            }
        }
    }
}

impl FusedIterator for Children<'_> {}

#[cfg(test)]
mod tests {
    use crate::raw_language::{RawLanguageKind, RawSyntaxTreeBuilder};
    use crate::GreenNode;

    fn build_test_list() -> GreenNode {
        let mut builder: RawSyntaxTreeBuilder = RawSyntaxTreeBuilder::new();

        // list
        builder.start_node(RawLanguageKind::SEPARATED_EXPRESSION_LIST);

        // element 1
        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::STRING_TOKEN, "a");
        builder.finish_node();

        // Missing ,

        // element 2
        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::STRING_TOKEN, "b");
        builder.finish_node();

        builder.finish_node();

        builder.finish_green()
    }

    #[test]
    fn children() {
        let root = build_test_list();

        // Test that children skips missing
        assert_eq!(root.children().count(), 2);
        assert_eq!(
            root.children()
                .map(|child| child.element.to_string())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        // Slot 2 (index 1) is empty
        assert_eq!(
            root.children().map(|child| child.slot).collect::<Vec<_>>(),
            vec![0, 2]
        );

        // Same when reverse
        assert_eq!(
            root.children()
                .rev()
                .map(|child| child.slot)
                .collect::<Vec<_>>(),
            vec![2, 0]
        );
    }

    #[test]
    fn slots() {
        let root = build_test_list();

        // Has 3 slots, one is missing
        assert_eq!(root.slots().len(), 3);
    }
}
