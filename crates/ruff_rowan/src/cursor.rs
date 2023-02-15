//! Implementation of the cursors -- API for convenient access to syntax trees.
//!
//! Functional programmers will recognize that this module implements a zipper
//! for a purely functional (green) tree.
//!
//! A cursor node (`SyntaxNode`) points to a `GreenNode` and a parent
//! `SyntaxNode`. This allows cursor to provide iteration over both ancestors
//! and descendants, as well as a cheep access to absolute offset of the node in
//! file.
//!

// Implementation notes:
//
// The implementation is utterly and horribly unsafe. This whole module is an
// unsafety boundary. It is believed that the API here is, in principle, sound,
// but the implementation might have bugs.
//
// The core type is `NodeData` -- a heap-allocated reference counted object,
// which points to a green node or a green token, and to the parent `NodeData`.
// Publicly-exposed `SyntaxNode` and `SyntaxToken` own a reference to
// `NodeData`.
//
// `NodeData`s are transient, and are created and destroyed during tree
// traversals. In general, only currently referenced nodes and their ancestors
// are alive at any given moment.
//
// More specifically, `NodeData`'s ref count is equal to the number of
// outstanding `SyntaxNode` and `SyntaxToken` plus the number of children with
// non-zero ref counts. For example, if the user has only a single `SyntaxNode`
// pointing somewhere in the middle of the tree, then all `NodeData` on the path
// from that point towards the root have ref count equal to one.
//
// `NodeData` which doesn't have a parent (is a root) owns the corresponding
// green node or token, and is responsible for freeing it. For child `NodeData`
// however since they hold a strong reference to their parent node and thus
// to the root, their corresponding green node is guaranteed to be alive as
// a reference cycle to is know to exist (child `NodeData` -> root `NodeData`
// -> root `GreenNode` -> child `GreenNode`) and they can safely use a "weak
// reference" (raw pointer) to the corresponding green node as an optimization
// to avoid having to track atomic references on the traversal hot path

mod element;
mod node;
mod token;
mod trivia;

use std::{iter, ops};
use std::{ptr, rc::Rc};

use countme::Count;
pub(crate) use trivia::{SyntaxTrivia, SyntaxTriviaPiecesIterator};

use crate::cursor::node::Siblings;
pub(crate) use crate::cursor::token::SyntaxToken;
use crate::green::{self, GreenElement, GreenNodeData, GreenTokenData};
use crate::{
    green::{GreenElementRef, RawSyntaxKind},
    NodeOrToken, TextRange, TextSize,
};
pub(crate) use element::SyntaxElement;
pub(crate) use node::{
    Preorder, PreorderWithTokens, SyntaxElementChildren, SyntaxNode, SyntaxNodeChildren,
    SyntaxSlot, SyntaxSlots,
};

#[derive(Debug)]
struct _SyntaxElement;

pub(crate) fn has_live() -> bool {
    countme::get::<_SyntaxElement>().live > 0
}

#[derive(Debug)]
struct NodeData {
    _c: Count<_SyntaxElement>,

    kind: NodeKind,
    slot: u32,

    /// Absolute offset for immutable nodes, unused for mutable nodes.
    offset: TextSize,
}

/// A single NodeData (red node) is either a "root node" (no parent node and
/// holds a strong reference to the root of the green tree) or a "child node"
/// (holds a strong reference to its parent red node and a weak reference to its
/// counterpart green node)
#[derive(Debug)]
enum NodeKind {
    Root {
        green: GreenElement,
    },
    Child {
        green: WeakGreenElement,
        parent: Rc<NodeData>,
    },
}

/// Child SyntaxNodes use "unsafe" weak pointers to refer to their green node.
/// Unlike the safe [std::sync::Weak] these are just a raw pointer: the
/// corresponding [ThinArc](crate::arc::ThinArc) doesn't keep a counter of
/// outstanding weak references or defer the release of the underlying memory
/// until the last `Weak` is dropped. On the other hand, a weak reference to a
/// released green node points to deallocated memory and it is undefined
/// behavior to dereference it, but in the context of `NodeData` this is
/// statically known to never happen
#[derive(Debug, Clone)]
enum WeakGreenElement {
    Node { ptr: ptr::NonNull<GreenNodeData> },
    Token { ptr: ptr::NonNull<GreenTokenData> },
}

impl WeakGreenElement {
    fn new(green: GreenElementRef) -> Self {
        match green {
            NodeOrToken::Node(ptr) => Self::Node {
                ptr: ptr::NonNull::from(ptr),
            },
            NodeOrToken::Token(ptr) => Self::Token {
                ptr: ptr::NonNull::from(ptr),
            },
        }
    }

    fn as_deref(&self) -> GreenElementRef {
        match self {
            WeakGreenElement::Node { ptr } => GreenElementRef::Node(unsafe { ptr.as_ref() }),
            WeakGreenElement::Token { ptr } => GreenElementRef::Token(unsafe { ptr.as_ref() }),
        }
    }

    fn to_owned(&self) -> GreenElement {
        match self {
            WeakGreenElement::Node { ptr } => {
                GreenElement::Node(unsafe { ptr.as_ref().to_owned() })
            }
            WeakGreenElement::Token { ptr } => {
                GreenElement::Token(unsafe { ptr.as_ref().to_owned() })
            }
        }
    }
}

impl NodeData {
    #[inline]
    fn new(kind: NodeKind, slot: u32, offset: TextSize) -> Rc<NodeData> {
        let res = NodeData {
            _c: Count::new(),
            kind,
            slot,
            offset,
        };

        Rc::new(res)
    }

    #[inline]
    fn key(&self) -> (ptr::NonNull<()>, TextSize) {
        let weak = match &self.kind {
            NodeKind::Root { green } => WeakGreenElement::new(green.as_deref()),
            NodeKind::Child { green, .. } => green.clone(),
        };
        let ptr = match weak {
            WeakGreenElement::Node { ptr } => ptr.cast(),
            WeakGreenElement::Token { ptr } => ptr.cast(),
        };
        (ptr, self.offset())
    }

    #[inline]
    fn parent_node(&self) -> Option<SyntaxNode> {
        debug_assert!(matches!(
            self.parent()?.green(),
            GreenElementRef::Node { .. }
        ));
        match &self.kind {
            NodeKind::Child { parent, .. } => Some(SyntaxNode {
                ptr: parent.clone(),
            }),
            NodeKind::Root { .. } => None,
        }
    }

    #[inline]
    fn parent(&self) -> Option<&NodeData> {
        match &self.kind {
            NodeKind::Child { parent, .. } => Some(&**parent),
            NodeKind::Root { .. } => None,
        }
    }

    #[inline]
    fn green(&self) -> GreenElementRef<'_> {
        match &self.kind {
            NodeKind::Root { green } => green.as_deref(),
            NodeKind::Child { green, .. } => green.as_deref(),
        }
    }

    /// Returns an iterator over the siblings of this node. The iterator is positioned at the current node.
    #[inline]
    fn green_siblings(&self) -> Option<Siblings> {
        match &self.parent()?.green() {
            GreenElementRef::Node(ptr) => Some(Siblings::new(ptr, self.slot())),
            GreenElementRef::Token(_) => {
                debug_assert!(
                    false,
                    "A token should never be a parent of a token or node."
                );
                None
            }
        }
    }
    #[inline]
    fn slot(&self) -> u32 {
        self.slot
    }

    #[inline]
    fn offset(&self) -> TextSize {
        self.offset
    }

    #[inline]
    fn text_range(&self) -> TextRange {
        let offset = self.offset();
        let len = self.green().text_len();
        TextRange::at(offset, len)
    }

    #[inline]
    fn kind(&self) -> RawSyntaxKind {
        self.green().kind()
    }

    fn next_sibling(&self) -> Option<SyntaxNode> {
        let siblings = self.green_siblings()?;
        siblings.following().find_map(|child| {
            child.element().into_node().and_then(|green| {
                let parent = self.parent_node()?;
                let offset = parent.offset() + child.rel_offset();
                Some(SyntaxNode::new_child(green, parent, child.slot(), offset))
            })
        })
    }
    fn prev_sibling(&self) -> Option<SyntaxNode> {
        let siblings = self.green_siblings()?;
        siblings.previous().find_map(|child| {
            child.element().into_node().and_then(|green| {
                let parent = self.parent_node()?;
                let offset = parent.offset() + child.rel_offset();
                Some(SyntaxNode::new_child(green, parent, child.slot(), offset))
            })
        })
    }

    fn next_sibling_or_token(&self) -> Option<SyntaxElement> {
        let siblings = self.green_siblings()?;

        siblings.following().next().and_then(|child| {
            let parent = self.parent_node()?;
            let offset = parent.offset() + child.rel_offset();
            Some(SyntaxElement::new(
                child.element(),
                parent,
                child.slot(),
                offset,
            ))
        })
    }
    fn prev_sibling_or_token(&self) -> Option<SyntaxElement> {
        let siblings = self.green_siblings()?;

        siblings.previous().next().and_then(|child| {
            let parent = self.parent_node()?;
            let offset = parent.offset() + child.rel_offset();
            Some(SyntaxElement::new(
                child.element(),
                parent,
                child.slot(),
                offset,
            ))
        })
    }

    fn into_green(self: Rc<Self>) -> GreenElement {
        match Rc::try_unwrap(self) {
            Ok(data) => match data.kind {
                NodeKind::Root { green } => green,
                NodeKind::Child { green, .. } => green.to_owned(),
            },
            Err(ptr) => ptr.green().to_owned(),
        }
    }

    /// Return a clone of this subtree detached from its parent
    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    fn detach(self: Rc<Self>) -> Rc<Self> {
        match &self.kind {
            NodeKind::Child { green, .. } => Self::new(
                NodeKind::Root {
                    green: green.to_owned(),
                },
                0,
                0.into(),
            ),
            // If this node is already detached, increment the reference count and return a clone
            NodeKind::Root { .. } => self.clone(),
        }
    }

    /// Return a clone of this node with the specified range of slots replaced
    /// with the elements of the provided iterator
    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    fn splice_slots<R, I>(mut self: Rc<Self>, range: R, replace_with: I) -> Rc<Self>
    where
        R: ops::RangeBounds<usize>,
        I: Iterator<Item = Option<green::GreenElement>>,
    {
        let green = match self.green() {
            NodeOrToken::Node(green) => green.splice_slots(range, replace_with).into(),
            NodeOrToken::Token(_) => panic!("called splice_slots on a token node"),
        };

        // Try to reuse the underlying memory allocation if self is the only
        // outstanding reference to this NodeData
        match Rc::get_mut(&mut self) {
            Some(node) => {
                node.kind = NodeKind::Root { green };
                node.slot = 0;
                node.offset = TextSize::from(0);
                self
            }
            None => Self::new(NodeKind::Root { green }, 0, 0.into()),
        }
    }

    /// Return a new version of this node with the element `prev_elem` replaced with `next_elem`
    ///
    /// `prev_elem` can be a direct child of this node, or an indirect child through any descendant node
    ///
    /// Returns `None` if `prev_elem` is not a descendant of this node
    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    fn replace_child(
        mut self: Rc<Self>,
        prev_elem: SyntaxElement,
        next_elem: SyntaxElement,
    ) -> Option<Rc<Self>> {
        let mut green = next_elem.into_green();
        let mut elem = prev_elem;

        loop {
            let node = elem.parent()?;
            let is_self = node.key() == self.key();

            let index = elem.index();
            let range = index..=index;

            let replace_with = iter::once(Some(green));
            green = node.green().splice_slots(range, replace_with).into();
            elem = node.into();

            if is_self {
                break;
            }
        }

        // Try to reuse the underlying memory allocation if self is the only
        // outstanding reference to this NodeData
        let result = match Rc::get_mut(&mut self) {
            Some(node) => {
                node.kind = NodeKind::Root { green };
                node.slot = 0;
                node.offset = TextSize::from(0);
                self
            }
            None => Self::new(NodeKind::Root { green }, 0, 0.into()),
        };

        Some(result)
    }
}
