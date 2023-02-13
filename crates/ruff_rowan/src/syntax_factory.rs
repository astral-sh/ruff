mod parsed_children;
mod raw_syntax;

use crate::SyntaxKind;
use std::fmt;
use std::iter::{FusedIterator, Peekable};

pub use self::parsed_children::{
    ParsedChildren, ParsedChildrenIntoIterator, ParsedChildrenIterator,
};
pub use self::raw_syntax::{
    RawSyntaxElement, RawSyntaxElementRef, RawSyntaxNode, RawSyntaxNodeRef, RawSyntaxToken,
    RawSyntaxTokenRef,
};

/// Factory for creating syntax nodes of a particular kind.
pub trait SyntaxFactory: fmt::Debug {
    /// The syntax kind used by the nodes constructed by this syntax factory.
    type Kind: SyntaxKind;

    /// Creates a new syntax node of the passed `kind` with the given children.
    ///
    /// The `children` contains the parsed direct children of the node. There may be fewer children
    /// in case there's a syntax error and a required child or an optional child isn't present in the source code.
    /// The `make_syntax` implementation must then fill in empty slots to match the slots as they're defined in the grammar.
    ///
    /// The implementation is free to change the `kind` of the node but that has the consequence that
    /// such a node will not be cached. The reason for not caching these nodes is that the cache lookup is performed
    /// before calling `make_syntax`, thus querying the cache with the old kind.
    ///
    /// It's important that the factory function is idempotent, meaning, calling the function
    /// multiple times with the same `kind` and `children` returns syntax nodes with the same structure.
    /// This is important because the returned nodes may be cached by `kind` and what `children` are present.
    fn make_syntax(
        kind: Self::Kind,
        children: ParsedChildren<Self::Kind>,
    ) -> RawSyntaxNode<Self::Kind>;

    /// Crates a *node list* syntax node. Validates if all elements are valid and changes the node's kind to
    /// [SyntaxKind::to_bogus] if that's not the case.
    fn make_node_list_syntax<F>(
        kind: Self::Kind,
        children: ParsedChildren<Self::Kind>,
        can_cast: F,
    ) -> RawSyntaxNode<Self::Kind>
    where
        F: Fn(Self::Kind) -> bool,
    {
        let valid = (&children)
            .into_iter()
            .all(|element| can_cast(element.kind()));

        let kind = if valid { kind } else { kind.to_bogus() };

        RawSyntaxNode::new(kind, children.into_iter().map(Some))
    }

    /// Creates a *separated list* syntax node. Validates if the elements are valid, are correctly
    /// separated by the specified separator token.
    ///
    /// It changes the kind of the node to [SyntaxKind::to_bogus] if an element isn't a valid list-node
    /// nor separator.
    ///
    /// It inserts empty slots for missing elements or missing markers
    fn make_separated_list_syntax<F>(
        kind: Self::Kind,
        children: ParsedChildren<Self::Kind>,
        can_cast: F,
        separator: Self::Kind,
        allow_trailing: bool,
    ) -> RawSyntaxNode<Self::Kind>
    where
        F: Fn(Self::Kind) -> bool,
    {
        let mut next_node = true;
        let mut missing_count = 0;
        let mut valid = true;

        for child in &children {
            let kind = child.kind();

            if next_node {
                if can_cast(kind) {
                    next_node = false;
                } else if kind == separator {
                    // a missing element
                    missing_count += 1;
                } else {
                    // an invalid element
                    valid = false;
                    break;
                }
            } else if kind == separator {
                next_node = true;
            } else if can_cast(kind) {
                // a missing separator
                missing_count += 1;
            } else {
                // something unexpected
                valid = false;
            }
        }

        if next_node && !allow_trailing && !children.is_empty() {
            // a trailing comma in a list that doesn't support trailing commas
            missing_count += 1;
        }

        if !valid {
            RawSyntaxNode::new(kind.to_bogus(), children.into_iter().map(Some))
        } else if missing_count > 0 {
            RawSyntaxNode::new(
                kind,
                SeparatedListWithMissingNodesOrSeparatorSlotsIterator {
                    inner: children.into_iter().peekable(),
                    missing_count,
                    next_node: true,
                    separator,
                },
            )
        } else {
            RawSyntaxNode::new(kind, children.into_iter().map(Some))
        }
    }
}

/// Iterator that "fixes up" a separated list by inserting empty slots for any missing
/// separator or element.
struct SeparatedListWithMissingNodesOrSeparatorSlotsIterator<'a, K: SyntaxKind> {
    inner: Peekable<ParsedChildrenIntoIterator<'a, K>>,
    missing_count: usize,
    next_node: bool,
    separator: K,
}

impl<'a, K: SyntaxKind> Iterator for SeparatedListWithMissingNodesOrSeparatorSlotsIterator<'a, K> {
    type Item = Option<RawSyntaxElement<K>>;

    #[cold]
    fn next(&mut self) -> Option<Self::Item> {
        let peeked = self.inner.peek();

        if let Some(peeked) = peeked {
            let is_separator = self.separator == peeked.kind();

            if self.next_node {
                self.next_node = false;
                if !is_separator {
                    Some(self.inner.next())
                } else {
                    self.missing_count -= 1;
                    Some(None) // Missing separator
                }
            } else if is_separator {
                self.next_node = true;
                Some(self.inner.next())
            } else {
                // Missing node
                self.missing_count -= 1;
                self.next_node = true;
                Some(None)
            }
        } else if self.missing_count > 0 {
            // at a trailing comma in a list that doesn't allow trailing commas.
            self.missing_count -= 1;
            Some(None)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'a, K: SyntaxKind> FusedIterator
    for SeparatedListWithMissingNodesOrSeparatorSlotsIterator<'a, K>
{
}

impl<'a, K: SyntaxKind> ExactSizeIterator
    for SeparatedListWithMissingNodesOrSeparatorSlotsIterator<'a, K>
{
    fn len(&self) -> usize {
        self.inner.len() + self.missing_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotContent {
    Present,
    Absent,
}

/// Description of the slots of a node in combination with [ParsedChildren].
/// It stores for each slot if the node is present in [ParsedChildren] or not, allowing
/// to generate a node with the right number of empty slots.
#[derive(Debug)]
pub struct RawNodeSlots<const COUNT: usize> {
    slots: [SlotContent; COUNT],
    current_slot: usize,
}

impl<const COUNT: usize> Default for RawNodeSlots<COUNT> {
    fn default() -> Self {
        Self {
            slots: [SlotContent::Absent; COUNT],
            current_slot: 0,
        }
    }
}

impl<const COUNT: usize> RawNodeSlots<COUNT> {
    /// Progresses to the next slot
    pub fn next_slot(&mut self) {
        debug_assert!(self.current_slot < COUNT);

        self.current_slot += 1;
    }

    /// Marks that the node for the current slot is *present* in the source code.
    pub fn mark_present(&mut self) {
        debug_assert!(self.current_slot < COUNT);

        self.slots[self.current_slot] = SlotContent::Present;
    }

    /// Creates a node with the kind `kind`, filling in the nodes from the `children`.
    pub fn into_node<K: SyntaxKind>(
        self,
        kind: K,
        children: ParsedChildren<K>,
    ) -> RawSyntaxNode<K> {
        debug_assert!(self.current_slot == COUNT, "Missing slots");

        RawSyntaxNode::new(
            kind,
            RawNodeSlotIterator {
                children: children.into_iter(),
                slots: self.slots.as_slice().iter(),
            },
        )
    }
}

struct RawNodeSlotIterator<'a, K: SyntaxKind> {
    children: ParsedChildrenIntoIterator<'a, K>,
    slots: std::slice::Iter<'a, SlotContent>,
}

impl<'a, K: SyntaxKind> Iterator for RawNodeSlotIterator<'a, K> {
    type Item = Option<RawSyntaxElement<K>>;

    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.slots.next()?;

        match slot {
            SlotContent::Present => {
                Some(Some(self.children.next().expect(
                    "Expected a present node according to the slot description",
                )))
            }
            SlotContent::Absent => Some(None),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.slots.len(), Some(self.slots.len()))
    }
}

impl<'a, K: SyntaxKind> FusedIterator for RawNodeSlotIterator<'a, K> {}
impl<'a, K: SyntaxKind> ExactSizeIterator for RawNodeSlotIterator<'a, K> {}
