use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::AnyNodeRef;
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

    let minimal = visitor.ancestors.pop().unwrap_or(root);
    CoveringNode {
        node: minimal,
        ancestors: visitor.ancestors,
    }
}

/// The node with a minimal range that fully contains the search range.
pub(crate) struct CoveringNode<'a> {
    /// The node with a minimal range that fully contains the search range.
    node: AnyNodeRef<'a>,

    /// The node's ancestor (the spine up to the root).
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> CoveringNode<'a> {
    pub(crate) fn node(&self) -> AnyNodeRef<'a> {
        self.node
    }

    /// Returns the node's parent.
    pub(crate) fn parent(&self) -> Option<AnyNodeRef<'a>> {
        self.ancestors.last().copied()
    }

    /// Finds the minimal node that fully covers the range and fulfills the given predicate.
    pub(crate) fn find(mut self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Result<Self, Self> {
        if f(self.node) {
            return Ok(self);
        }

        match self.ancestors.iter().rposition(|node| f(*node)) {
            Some(index) => {
                let node = self.ancestors[index];
                self.ancestors.truncate(index);

                Ok(Self {
                    node,
                    ancestors: self.ancestors,
                })
            }
            None => Err(self),
        }
    }
}

impl fmt::Debug for CoveringNode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeWithAncestors")
            .field(&self.node)
            .finish()
    }
}
