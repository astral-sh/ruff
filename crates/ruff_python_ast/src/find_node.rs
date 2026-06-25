use crate::AnyNodeRef;
use crate::visitor::source_order::{SourceOrderVisitor, TraversalSignal, walk_node};
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::fmt::Formatter;

/// Returns the node with a minimal range that fully contains `range`.
///
/// If `range` is empty and falls within a parser *synthesized* node generated during error recovery,
/// then the first node with the given range is returned.
///
/// ## Panics
/// Panics if `range` is not contained within `root`.
pub fn covering_node(root: AnyNodeRef, range: TextRange) -> CoveringNode {
    struct Visitor<'a> {
        range: TextRange,
        found: bool,
        ancestors: Vec<AnyNodeRef<'a>>,
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            // If the node fully contains the range, than it is a possible match but traverse into its children
            // to see if there's a node with a narrower range.
            if !self.found && node.range().contains_range(self.range) {
                self.ancestors.push(node);
                TraversalSignal::Traverse
            } else {
                TraversalSignal::Skip
            }
        }

        fn leave_node(&mut self, node: AnyNodeRef<'a>) {
            if !self.found && self.ancestors.last() == Some(&node) {
                self.found = true;
            }
        }
    }

    assert!(
        root.range().contains_range(range),
        "Range is not contained within root"
    );

    let mut visitor = Visitor {
        range,
        found: false,
        ancestors: Vec::new(),
    };

    walk_node(&mut visitor, root);
    CoveringNode::from_ancestors(visitor.ancestors)
}

/// The node with a minimal range that fully contains the search range.
pub struct CoveringNode<'a> {
    /// The covering node, along with all of its ancestors up to the
    /// root. The root is always the first element and the covering
    /// node found is always the last node. This sequence is guaranteed
    /// to be non-empty.
    nodes: Vec<AnyNodeRef<'a>>,
}

impl<'a> CoveringNode<'a> {
    /// Creates a new `CoveringNode` from a list of ancestor nodes.
    /// The ancestors should be ordered from root to the covering node.
    pub fn from_ancestors(ancestors: Vec<AnyNodeRef<'a>>) -> Self {
        Self { nodes: ancestors }
    }

    /// Returns the covering node found.
    pub fn node(&self) -> AnyNodeRef<'a> {
        *self
            .nodes
            .last()
            .expect("`CoveringNode::nodes` should always be non-empty")
    }

    /// Returns the node's parent.
    pub fn parent(&self) -> Option<AnyNodeRef<'a>> {
        let penultimate = self.nodes.len().checked_sub(2)?;
        self.nodes.get(penultimate).copied()
    }

    /// Finds the first node that fully covers the range and fulfills
    /// the given predicate.
    ///
    /// The "first" here means that the node closest to a leaf is
    /// returned.
    pub fn find_first(mut self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Result<Self, Self> {
        let Some(index) = self.find_first_index(f) else {
            return Err(self);
        };
        self.nodes.truncate(index + 1);
        Ok(self)
    }

    /// Finds the last node that fully covers the range and fulfills
    /// the given predicate.
    ///
    /// The "last" here means that after finding the "first" such node,
    /// the highest ancestor found satisfying the given predicate is
    /// returned. Note that this is *not* the same as finding the node
    /// closest to the root that satisfies the given predictate.
    pub fn find_last(mut self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Result<Self, Self> {
        let Some(mut index) = self.find_first_index(&f) else {
            return Err(self);
        };
        while index > 0 && f(self.nodes[index - 1]) {
            index -= 1;
        }
        self.nodes.truncate(index + 1);
        Ok(self)
    }

    /// Returns an iterator over the ancestor nodes, starting with the node itself
    /// and walking towards the root.
    pub fn ancestors(&self) -> impl DoubleEndedIterator<Item = AnyNodeRef<'a>> + '_ {
        self.nodes.iter().copied().rev()
    }

    /// Finds the index of the node that fully covers the range and
    /// fulfills the given predicate.
    ///
    /// If there are no nodes matching the given predictate, then
    /// `None` is returned.
    fn find_first_index(&self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Option<usize> {
        self.nodes.iter().rposition(|node| f(*node))
    }
}

impl fmt::Debug for CoveringNode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CoveringNode").field(&self.node()).finish()
    }
}
