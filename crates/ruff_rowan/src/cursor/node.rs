use crate::cursor::{NodeData, SyntaxElement, SyntaxToken, SyntaxTrivia};
use crate::green::{Child, Children, GreenElementRef, Slot};
use crate::{
    Direction, GreenNode, GreenNodeData, NodeOrToken, RawSyntaxKind, SyntaxNodeText, TokenAtOffset,
    WalkEvent,
};
use ruff_text_size::{TextRange, TextSize};
use std::hash::{Hash, Hasher};
use std::iter::FusedIterator;
use std::ops;
use std::ptr::NonNull;
use std::rc::Rc;
use std::{fmt, iter};

use super::{GreenElement, NodeKind, WeakGreenElement};

#[derive(Clone)]
pub(crate) struct SyntaxNode {
    pub(super) ptr: Rc<NodeData>,
}

impl SyntaxNode {
    pub(crate) fn new_root(green: GreenNode) -> SyntaxNode {
        SyntaxNode {
            ptr: NodeData::new(
                NodeKind::Root {
                    green: GreenElement::Node(green),
                },
                0,
                0.into(),
            ),
        }
    }

    pub(super) fn new_child(
        green: &GreenNodeData,
        parent: SyntaxNode,
        slot: u32,
        offset: TextSize,
    ) -> SyntaxNode {
        SyntaxNode {
            ptr: NodeData::new(
                NodeKind::Child {
                    green: WeakGreenElement::new(GreenElementRef::Node(green)),
                    parent: parent.ptr,
                },
                slot,
                offset,
            ),
        }
    }

    pub fn clone_subtree(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green().into())
    }

    #[inline]
    pub(super) fn data(&self) -> &NodeData {
        self.ptr.as_ref()
    }

    #[inline]
    pub fn kind(&self) -> RawSyntaxKind {
        self.data().kind()
    }

    #[inline]
    pub(super) fn offset(&self) -> TextSize {
        self.data().offset()
    }

    pub(crate) fn element_in_slot(&self, slot_index: u32) -> Option<SyntaxElement> {
        let slot = self
            .slots()
            .nth(slot_index as usize)
            .expect("Slot index out of bounds");

        slot.map(|element| element)
    }

    #[inline]
    pub(crate) fn slots(&self) -> SyntaxSlots {
        SyntaxSlots::new(self.clone())
    }

    #[inline]
    pub fn text_range(&self) -> TextRange {
        self.data().text_range()
    }

    pub fn text_trimmed_range(&self) -> TextRange {
        let range = self.text_range();
        let mut start = range.start();
        let mut end = range.end();

        // Remove all trivia from the start of the node
        let mut token = self.first_token();
        while let Some(t) = token.take() {
            let (leading_len, trailing_len, total_len) = t.green().leading_trailing_total_len();
            let token_len: u32 = (total_len - leading_len - trailing_len).into();
            if token_len == 0 {
                start += total_len;
                token = t.next_token();
            } else {
                start += leading_len;
            }
        }

        // Remove all trivia from the end of the node
        let mut token = self.last_token();
        while let Some(t) = token.take() {
            let (leading_len, trailing_len, total_len) = t.green().leading_trailing_total_len();
            let token_len: u32 = (total_len - leading_len - trailing_len).into();
            if token_len == 0 {
                end -= total_len;
                token = t.prev_token();
            } else {
                end -= trailing_len;
            }
        }

        TextRange::new(start, end.max(start))
    }

    pub fn first_leading_trivia(&self) -> Option<SyntaxTrivia> {
        self.first_token().map(|x| x.leading_trivia())
    }

    pub fn last_trailing_trivia(&self) -> Option<SyntaxTrivia> {
        self.last_token().map(|x| x.trailing_trivia())
    }

    #[inline]
    pub fn index(&self) -> usize {
        self.data().slot() as usize
    }

    #[inline]
    pub fn text(&self) -> SyntaxNodeText {
        SyntaxNodeText::new(self.clone())
    }

    #[inline]
    pub fn text_trimmed(&self) -> SyntaxNodeText {
        SyntaxNodeText::with_range(self.clone(), self.text_trimmed_range())
    }

    #[inline]
    pub(crate) fn key(&self) -> (NonNull<()>, TextSize) {
        self.data().key()
    }

    #[inline]
    pub(crate) fn green(&self) -> &GreenNodeData {
        self.data().green().into_node().unwrap()
    }

    #[inline]
    pub fn parent(&self) -> Option<SyntaxNode> {
        self.data().parent_node()
    }

    #[inline]
    pub fn ancestors(&self) -> impl Iterator<Item = SyntaxNode> {
        iter::successors(Some(self.clone()), SyntaxNode::parent)
    }

    #[inline]
    pub fn children(&self) -> SyntaxNodeChildren {
        SyntaxNodeChildren::new(self.clone())
    }

    #[inline]
    pub fn children_with_tokens(&self) -> SyntaxElementChildren {
        SyntaxElementChildren::new(self.clone())
    }

    #[inline]
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + DoubleEndedIterator + '_ {
        self.green().children().filter_map(|child| {
            child.element().into_token().map(|token| {
                SyntaxToken::new(
                    token,
                    self.clone(),
                    child.slot(),
                    self.offset() + child.rel_offset(),
                )
            })
        })
    }

    pub fn first_child(&self) -> Option<SyntaxNode> {
        self.green().children().find_map(|child| {
            child.element().into_node().map(|green| {
                SyntaxNode::new_child(
                    green,
                    self.clone(),
                    child.slot(),
                    self.offset() + child.rel_offset(),
                )
            })
        })
    }

    pub fn last_child(&self) -> Option<SyntaxNode> {
        self.green().children().rev().find_map(|child| {
            child.element().into_node().map(|green| {
                SyntaxNode::new_child(
                    green,
                    self.clone(),
                    child.slot(),
                    self.offset() + child.rel_offset(),
                )
            })
        })
    }

    pub fn first_child_or_token(&self) -> Option<SyntaxElement> {
        self.green().children().next().map(|child| {
            SyntaxElement::new(
                child.element(),
                self.clone(),
                child.slot(),
                self.offset() + child.rel_offset(),
            )
        })
    }
    pub fn last_child_or_token(&self) -> Option<SyntaxElement> {
        self.green().children().next_back().map(|child| {
            SyntaxElement::new(
                child.element(),
                self.clone(),
                child.slot(),
                self.offset() + child.rel_offset(),
            )
        })
    }

    pub fn next_sibling(&self) -> Option<SyntaxNode> {
        self.data().next_sibling()
    }
    pub fn prev_sibling(&self) -> Option<SyntaxNode> {
        self.data().prev_sibling()
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        self.data().next_sibling_or_token()
    }
    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        self.data().prev_sibling_or_token()
    }

    pub fn first_token(&self) -> Option<SyntaxToken> {
        self.descendants_with_tokens(Direction::Next)
            .find_map(|x| x.into_token())
    }

    pub fn last_token(&self) -> Option<SyntaxToken> {
        PreorderWithTokens::new(self.clone(), Direction::Prev)
            .filter_map(|event| match event {
                WalkEvent::Enter(it) => Some(it),
                WalkEvent::Leave(_) => None,
            })
            .find_map(|x| x.into_token())
    }

    #[inline]
    pub fn siblings(&self, direction: Direction) -> impl Iterator<Item = SyntaxNode> {
        iter::successors(Some(self.clone()), move |node| match direction {
            Direction::Next => node.next_sibling(),
            Direction::Prev => node.prev_sibling(),
        })
    }

    #[inline]
    pub fn siblings_with_tokens(
        &self,
        direction: Direction,
    ) -> impl Iterator<Item = SyntaxElement> {
        let me: SyntaxElement = self.clone().into();
        iter::successors(Some(me), move |el| match direction {
            Direction::Next => el.next_sibling_or_token(),
            Direction::Prev => el.prev_sibling_or_token(),
        })
    }

    #[inline]
    pub fn descendants(&self) -> impl Iterator<Item = SyntaxNode> {
        self.preorder().filter_map(|event| match event {
            WalkEvent::Enter(node) => Some(node),
            WalkEvent::Leave(_) => None,
        })
    }

    #[inline]
    pub fn descendants_with_tokens(
        &self,
        direction: Direction,
    ) -> impl Iterator<Item = SyntaxElement> {
        self.preorder_with_tokens(direction)
            .filter_map(|event| match event {
                WalkEvent::Enter(it) => Some(it),
                WalkEvent::Leave(_) => None,
            })
    }

    #[inline]
    pub fn preorder(&self) -> Preorder {
        Preorder::new(self.clone())
    }

    #[inline]
    pub fn preorder_with_tokens(&self, direction: Direction) -> PreorderWithTokens {
        PreorderWithTokens::new(self.clone(), direction)
    }

    pub(crate) fn preorder_slots(&self) -> SlotsPreorder {
        SlotsPreorder::new(self.clone())
    }

    pub fn token_at_offset(&self, offset: TextSize) -> TokenAtOffset<SyntaxToken> {
        // TODO: this could be faster if we first drill-down to node, and only
        // then switch to token search. We should also replace explicit
        // recursion with a loop.
        let range = self.text_range();
        assert!(
            range.start() <= offset && offset <= range.end(),
            "Bad offset: range {:?} offset {:?}",
            range,
            offset
        );
        if range.is_empty() {
            return TokenAtOffset::None;
        }

        let mut children = self.children_with_tokens().filter(|child| {
            let child_range = child.text_range();
            !child_range.is_empty() && child_range.contains_inclusive(offset)
        });

        let left = children.next().unwrap();
        let right = children.next();
        assert!(children.next().is_none());

        if let Some(right) = right {
            match (left.token_at_offset(offset), right.token_at_offset(offset)) {
                (TokenAtOffset::Single(left), TokenAtOffset::Single(right)) => {
                    TokenAtOffset::Between(left, right)
                }
                _ => unreachable!(),
            }
        } else {
            left.token_at_offset(offset)
        }
    }

    pub fn covering_element(&self, range: TextRange) -> SyntaxElement {
        let mut res: SyntaxElement = self.clone().into();
        loop {
            assert!(
                res.text_range().contains_range(range),
                "Bad range: node range {:?}, range {:?}",
                res.text_range(),
                range,
            );
            res = match &res {
                NodeOrToken::Token(_) => return res,
                NodeOrToken::Node(node) => match node.child_or_token_at_range(range) {
                    Some(it) => it,
                    None => return res,
                },
            };
        }
    }

    pub fn child_or_token_at_range(&self, range: TextRange) -> Option<SyntaxElement> {
        let rel_range = range - self.offset();
        self.green()
            .slot_at_range(rel_range)
            .and_then(|(index, rel_offset, slot)| {
                slot.as_ref().map(|green| {
                    SyntaxElement::new(
                        green,
                        self.clone(),
                        index as u32,
                        self.offset() + rel_offset,
                    )
                })
            })
    }

    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    pub fn detach(self) -> Self {
        Self {
            ptr: self.ptr.detach(),
        }
    }

    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    pub fn splice_slots<R, I>(self, range: R, replace_with: I) -> Self
    where
        R: ops::RangeBounds<usize>,
        I: Iterator<Item = Option<SyntaxElement>>,
    {
        Self {
            ptr: self.ptr.splice_slots(
                range,
                replace_with.into_iter().map(|element| {
                    element.map(|child| match child.detach() {
                        NodeOrToken::Node(it) => it.ptr.into_green(),
                        NodeOrToken::Token(it) => it.into_green(),
                    })
                }),
            ),
        }
    }

    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    pub fn replace_child(self, prev_elem: SyntaxElement, next_elem: SyntaxElement) -> Option<Self> {
        Some(Self {
            ptr: self.ptr.replace_child(prev_elem, next_elem)?,
        })
    }
}

// Identity semantics for hash & eq
impl PartialEq for SyntaxNode {
    #[inline]
    fn eq(&self, other: &SyntaxNode) -> bool {
        self.data().key() == other.data().key()
    }
}

impl Eq for SyntaxNode {}

impl Hash for SyntaxNode {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data().key().hash(state);
    }
}

impl fmt::Debug for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxNode")
            .field("kind", &self.kind())
            .field("text_range", &self.text_range())
            .finish()
    }
}

impl fmt::Display for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.preorder_with_tokens(Direction::Next)
            .filter_map(|event| match event {
                WalkEvent::Enter(NodeOrToken::Token(token)) => Some(token),
                _ => None,
            })
            .try_for_each(|it| fmt::Display::fmt(&it, f))
    }
}

// region: iterators

#[derive(Clone, Debug)]
pub(crate) struct SyntaxNodeChildren {
    next: Option<SyntaxNode>,
}

impl SyntaxNodeChildren {
    fn new(parent: SyntaxNode) -> SyntaxNodeChildren {
        SyntaxNodeChildren {
            next: parent.first_child(),
        }
    }
}

impl Iterator for SyntaxNodeChildren {
    type Item = SyntaxNode;
    fn next(&mut self) -> Option<SyntaxNode> {
        self.next.take().map(|next| {
            self.next = next.next_sibling();
            next
        })
    }
}

impl FusedIterator for SyntaxNodeChildren {}

#[derive(Clone, Debug, Default)]
pub(crate) struct SyntaxElementChildren {
    next: Option<SyntaxElement>,
}

impl SyntaxElementChildren {
    fn new(parent: SyntaxNode) -> SyntaxElementChildren {
        SyntaxElementChildren {
            next: parent.first_child_or_token(),
        }
    }
}

impl Iterator for SyntaxElementChildren {
    type Item = SyntaxElement;
    fn next(&mut self) -> Option<SyntaxElement> {
        self.next.take().map(|next| {
            self.next = next.next_sibling_or_token();
            next
        })
    }
}

impl FusedIterator for SyntaxElementChildren {}

pub(crate) struct Preorder {
    start: SyntaxNode,
    next: Option<WalkEvent<SyntaxNode>>,
    skip_subtree: bool,
}

impl Preorder {
    fn new(start: SyntaxNode) -> Preorder {
        let next = Some(WalkEvent::Enter(start.clone()));
        Preorder {
            start,
            next,
            skip_subtree: false,
        }
    }

    pub fn skip_subtree(&mut self) {
        self.skip_subtree = true;
    }

    #[cold]
    fn do_skip(&mut self) {
        self.next = self.next.take().map(|next| match next {
            WalkEvent::Enter(first_child) => WalkEvent::Leave(first_child.parent().unwrap()),
            WalkEvent::Leave(parent) => WalkEvent::Leave(parent),
        })
    }
}

impl Iterator for Preorder {
    type Item = WalkEvent<SyntaxNode>;

    fn next(&mut self) -> Option<WalkEvent<SyntaxNode>> {
        if self.skip_subtree {
            self.do_skip();
            self.skip_subtree = false;
        }
        let next = self.next.take();
        self.next = next.as_ref().and_then(|next| {
            Some(match next {
                WalkEvent::Enter(node) => match node.first_child() {
                    Some(child) => WalkEvent::Enter(child),
                    None => WalkEvent::Leave(node.clone()),
                },
                WalkEvent::Leave(node) => {
                    if node == &self.start {
                        return None;
                    }
                    match node.next_sibling() {
                        Some(sibling) => WalkEvent::Enter(sibling),
                        None => WalkEvent::Leave(node.parent()?),
                    }
                }
            })
        });
        next
    }
}

impl FusedIterator for Preorder {}

pub(crate) struct PreorderWithTokens {
    start: SyntaxElement,
    next: Option<WalkEvent<SyntaxElement>>,
    skip_subtree: bool,
    direction: Direction,
}

impl PreorderWithTokens {
    fn new(start: SyntaxNode, direction: Direction) -> PreorderWithTokens {
        let next = Some(WalkEvent::Enter(start.clone().into()));
        PreorderWithTokens {
            start: start.into(),
            next,
            direction,
            skip_subtree: false,
        }
    }

    pub fn skip_subtree(&mut self) {
        self.skip_subtree = true;
    }

    #[cold]
    fn do_skip(&mut self) {
        self.next = self.next.take().map(|next| match next {
            WalkEvent::Enter(first_child) => WalkEvent::Leave(first_child.parent().unwrap().into()),
            WalkEvent::Leave(parent) => WalkEvent::Leave(parent),
        })
    }
}

impl Iterator for PreorderWithTokens {
    type Item = WalkEvent<SyntaxElement>;

    fn next(&mut self) -> Option<WalkEvent<SyntaxElement>> {
        if self.skip_subtree {
            self.do_skip();
            self.skip_subtree = false;
        }
        let next = self.next.take();
        self.next = next.as_ref().and_then(|next| {
            Some(match next {
                WalkEvent::Enter(el) => match el {
                    NodeOrToken::Node(node) => {
                        let next = match self.direction {
                            Direction::Next => node.first_child_or_token(),
                            Direction::Prev => node.last_child_or_token(),
                        };
                        match next {
                            Some(child) => WalkEvent::Enter(child),
                            None => WalkEvent::Leave(node.clone().into()),
                        }
                    }
                    NodeOrToken::Token(token) => WalkEvent::Leave(token.clone().into()),
                },
                WalkEvent::Leave(el) if el == &self.start => return None,
                WalkEvent::Leave(el) => {
                    let next = match self.direction {
                        Direction::Next => el.next_sibling_or_token(),
                        Direction::Prev => el.prev_sibling_or_token(),
                    };

                    match next {
                        Some(sibling) => WalkEvent::Enter(sibling),
                        None => WalkEvent::Leave(el.parent()?.into()),
                    }
                }
            })
        });
        next
    }
}

impl FusedIterator for PreorderWithTokens {}

/// Represents a cursor to a green node slot. A slot either contains an element or is empty
/// if the child isn't present in the source.
#[derive(Debug, Clone)]
pub(crate) enum SyntaxSlot {
    Node(SyntaxNode),
    Token(SyntaxToken),
    Empty { parent: SyntaxNode, index: u32 },
}

impl From<SyntaxElement> for SyntaxSlot {
    fn from(element: SyntaxElement) -> Self {
        match element {
            SyntaxElement::Node(node) => SyntaxSlot::Node(node),
            SyntaxElement::Token(token) => SyntaxSlot::Token(token),
        }
    }
}

impl SyntaxSlot {
    #[inline]
    pub fn map<F, R>(self, mapper: F) -> Option<R>
    where
        F: FnOnce(SyntaxElement) -> R,
    {
        match self {
            SyntaxSlot::Node(node) => Some(mapper(SyntaxElement::Node(node))),
            SyntaxSlot::Token(token) => Some(mapper(SyntaxElement::Token(token))),
            SyntaxSlot::Empty { .. } => None,
        }
    }
}

/// Iterator over a node's slots
#[derive(Debug, Clone)]
pub(crate) struct SyntaxSlots {
    /// Position of the next element to return.
    pos: u32,

    /// Position of the last returned element from the back.
    /// Initially points one element past the last slot.
    ///
    /// [nth_back]: https://doc.rust-lang.org/std/iter/trait.DoubleEndedIterator.html#method.nth_back
    back_pos: u32,
    parent: SyntaxNode,
}

impl SyntaxSlots {
    #[inline]
    fn new(parent: SyntaxNode) -> Self {
        Self {
            pos: 0,
            back_pos: parent.green().slice().len() as u32,
            parent,
        }
    }

    /// Returns a slice containing the remaining elements to iterate over
    /// an empty slice if the iterator reached the end.
    #[inline]
    fn slice(&self) -> &[Slot] {
        if self.pos < self.back_pos {
            &self.parent.green().slice()[self.pos as usize..self.back_pos as usize]
        } else {
            &[]
        }
    }

    fn map_slot(&self, slot: &Slot, slot_index: u32) -> SyntaxSlot {
        match slot {
            Slot::Empty { .. } => SyntaxSlot::Empty {
                parent: self.parent.clone(),
                index: slot_index,
            },
            Slot::Token { rel_offset, token } => SyntaxSlot::Token(SyntaxToken::new(
                token,
                self.parent.clone(),
                slot_index,
                self.parent.offset() + rel_offset,
            )),
            Slot::Node { rel_offset, node } => SyntaxSlot::Node(SyntaxNode::new_child(
                node,
                self.parent.clone(),
                slot_index,
                self.parent.offset() + rel_offset,
            )),
        }
    }
}

impl Iterator for SyntaxSlots {
    type Item = SyntaxSlot;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.slice().first()?;
        let mapped = self.map_slot(slot, self.pos);
        self.pos += 1;
        Some(mapped)
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.slice().len();
        (len, Some(len))
    }

    #[inline(always)]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.len()
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
        self.pos += n as u32;
        self.next()
    }
}

impl ExactSizeIterator for SyntaxSlots {
    #[inline(always)]
    fn len(&self) -> usize {
        self.slice().len()
    }
}

impl FusedIterator for SyntaxSlots {}

impl DoubleEndedIterator for SyntaxSlots {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let slot = self.slice().last()?;
        let mapped = self.map_slot(slot, self.back_pos - 1);
        self.back_pos -= 1;
        Some(mapped)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.back_pos -= n as u32;
        self.next_back()
    }
}

/// Iterator to visit a node's slots in pre-order.
pub(crate) struct SlotsPreorder {
    start: SyntaxNode,
    next: Option<WalkEvent<SyntaxSlot>>,
}

impl SlotsPreorder {
    fn new(start: SyntaxNode) -> Self {
        let next = Some(WalkEvent::Enter(SyntaxSlot::Node(start.clone())));
        SlotsPreorder { start, next }
    }
}

impl Iterator for SlotsPreorder {
    type Item = WalkEvent<SyntaxSlot>;

    fn next(&mut self) -> Option<WalkEvent<SyntaxSlot>> {
        let next = self.next.take();
        self.next = next.as_ref().and_then(|next| {
            Some(match next {
                WalkEvent::Enter(slot) => match slot {
                    SyntaxSlot::Empty { .. } | SyntaxSlot::Token(_) => {
                        WalkEvent::Leave(slot.clone())
                    }
                    SyntaxSlot::Node(node) => match node.slots().next() {
                        None => WalkEvent::Leave(SyntaxSlot::Node(node.clone())),
                        Some(first_slot) => WalkEvent::Enter(first_slot),
                    },
                },
                WalkEvent::Leave(slot) => {
                    let (parent, slot_index) = match slot {
                        SyntaxSlot::Empty { parent, index } => (parent.clone(), *index as usize),
                        SyntaxSlot::Token(token) => (token.parent()?, token.index()),
                        SyntaxSlot::Node(node) => {
                            if node == &self.start {
                                return None;
                            }

                            (node.parent()?, node.index())
                        }
                    };

                    let next_slot = parent.slots().nth(slot_index + 1);
                    match next_slot {
                        Some(slot) => WalkEvent::Enter(slot),
                        None => WalkEvent::Leave(SyntaxSlot::Node(parent)),
                    }
                }
            })
        });
        next
    }
}

impl FusedIterator for SlotsPreorder {}

#[derive(Debug, Clone)]
pub(crate) struct Siblings<'a> {
    parent: &'a GreenNodeData,
    start_slot: u32,
}

impl<'a> Siblings<'a> {
    pub fn new(parent: &'a GreenNodeData, start_slot: u32) -> Self {
        assert!(
            (start_slot as usize) < parent.slots().len(),
            "Start slot {} out of bounds {}",
            start_slot,
            parent.slots().len()
        );

        Self { parent, start_slot }
    }

    /// Creates an iterator over the siblings following the start node.
    /// For example, the following siblings of the if statement's condition are
    /// * the consequence
    /// * potentially the else clause
    pub fn following(&self) -> Children<'a> {
        let mut slots = self.parent.slots().enumerate();

        // Navigate to the start slot so that calling `next` returns the first following sibling
        slots.nth(self.start_slot as usize);

        Children::new(slots)
    }

    /// Creates an iterator over the siblings preceding the start node in reverse order.
    /// For example, the preceding siblings of the if statement's condition are:
    /// * opening parentheses: (
    /// * if keyword: if
    pub fn previous(&self) -> impl Iterator<Item = Child<'a>> {
        let mut slots = self.parent.slots().enumerate();

        // Navigate to the start slot from the back so that calling `next_back` (or rev().next()) returns
        // the first slot preceding the start node
        slots.nth_back(slots.len() - 1 - self.start_slot as usize);

        Children::new(slots).rev()
    }
}

// endregion

#[cfg(test)]
mod tests {
    use crate::raw_language::{RawLanguageKind, RawSyntaxTreeBuilder};

    #[test]
    fn slots_iter() {
        let mut builder = RawSyntaxTreeBuilder::new();
        builder.start_node(RawLanguageKind::EXPRESSION_LIST);

        for number in [1, 2, 3, 4] {
            builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
            builder.token(RawLanguageKind::NUMBER_TOKEN, &number.to_string());
            builder.finish_node();
        }
        builder.finish_node();

        let list = builder.finish();

        let mut iter = list.slots();

        assert_eq!(iter.size_hint(), (4, Some(4)));

        assert_eq!(
            iter.next()
                .and_then(|slot| slot.into_node())
                .map(|node| node.text().to_string())
                .as_deref(),
            Some("1")
        );

        assert_eq!(iter.size_hint(), (3, Some(3)));

        assert_eq!(
            iter.next_back()
                .and_then(|slot| slot.into_node())
                .map(|node| node.text().to_string())
                .as_deref(),
            Some("4")
        );

        assert_eq!(iter.size_hint(), (2, Some(2)));

        assert_eq!(
            iter.last()
                .and_then(|slot| slot.into_node())
                .map(|node| node.text().to_string())
                .as_deref(),
            Some("3")
        );
    }
}
