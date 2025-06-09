use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
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
pub(crate) fn covering_node(root: AnyNodeRef, range: TextRange) -> CoveringNode {
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

    root.visit_source_order(&mut visitor);
    if visitor.ancestors.is_empty() {
        visitor.ancestors.push(root);
    }
    CoveringNode {
        nodes: visitor.ancestors,
    }
}

/// The node with a minimal range that fully contains the search range.
pub(crate) struct CoveringNode<'a> {
    /// The covering node, along with all of its ancestors up to the
    /// root. The root is always the first element and the covering
    /// node found is always the last node. This sequence is guaranteed
    /// to be non-empty.
    nodes: Vec<AnyNodeRef<'a>>,
}

impl<'a> CoveringNode<'a> {
    /// Returns the covering node found.
    pub(crate) fn node(&self) -> AnyNodeRef<'a> {
        *self.nodes.last().unwrap()
    }

    /// Returns the node's parent.
    pub(crate) fn parent(&self) -> Option<AnyNodeRef<'a>> {
        let penultimate = self.nodes.len().checked_sub(2)?;
        self.nodes.get(penultimate).copied()
    }

    /// Finds the minimal node that fully covers the range and fulfills the given predicate.
    pub(crate) fn find(mut self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Result<Self, Self> {
        match self.nodes.iter().rposition(|node| f(*node)) {
            Some(index) => {
                self.nodes.truncate(index + 1);
                Ok(self)
            }
            None => Err(self),
        }
    }
}

impl fmt::Debug for CoveringNode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CoveringNode").field(&self.node()).finish()
    }
}
