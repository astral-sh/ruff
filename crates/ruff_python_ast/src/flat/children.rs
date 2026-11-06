use std::iter::FusedIterator;
use std::mem;
use std::num::NonZeroU32;
use std::slice;

use smallvec::SmallVec;

use super::{Ast, AstNode, AstNodeId, SyntaxNode};
use crate::Stmt;

/// The immediate children of a syntax node, in source order.
pub struct Children<'a> {
    ast: &'a Ast,
    source: ChildSource<'a>,
}

enum ChildSource<'a> {
    Empty,
    Rows(slice::Iter<'a, u32>),
    #[cfg(target_endian = "big")]
    InlineWords(&'a [u32]),
    Sequence {
        first: Option<AstNodeId<crate::kind::Any>>,
        rows: slice::Iter<'a, u32>,
        last: Option<AstNodeId<crate::kind::Any>>,
    },
    Fields(Fields<'a>),
}

struct Fields<'a> {
    node: AstNodeId<crate::kind::Any>,
    unvisited: std::ops::Range<u8>,
    rows: slice::Iter<'a, u32>,
}

/// A child field holds either a single node or a list of arena rows.
pub(super) enum Field<'a> {
    One(Option<AstNodeId<crate::kind::Any>>),
    Rows(&'a [u32]),
}

impl<'a> Fields<'a> {
    fn next(&mut self, ast: &'a Ast) -> Option<AstNodeId<crate::kind::Any>> {
        loop {
            if let Some(&word) = self.rows.next() {
                return Some(AstNodeId::from_word(word));
            }
            let field = self.unvisited.next()?;
            match ast.child_field(ast.at(self.node), field) {
                Field::One(Some(handle)) => return Some(handle),
                Field::One(None) => {}
                Field::Rows(words) => self.rows = words.iter(),
            }
        }
    }
}

impl<'a> Children<'a> {
    pub(super) fn empty(ast: &'a Ast) -> Self {
        Self {
            ast,
            source: ChildSource::Empty,
        }
    }

    /// Inline fields use little-endian integers. Lists in the extra buffer use native integers.
    #[inline]
    pub(super) fn inline(ast: &'a Ast, words: &'a [u32]) -> Self {
        #[cfg(target_endian = "little")]
        let source = ChildSource::Rows(words.iter());
        #[cfg(target_endian = "big")]
        let source = ChildSource::InlineWords(words);
        Self { ast, source }
    }

    pub(super) fn sequence(
        ast: &'a Ast,
        first: Option<AstNodeId<crate::kind::Any>>,
        rows: &'a [u32],
        last: Option<AstNodeId<crate::kind::Any>>,
    ) -> Self {
        Self {
            ast,
            source: ChildSource::Sequence {
                first,
                rows: rows.iter(),
                last,
            },
        }
    }

    #[inline]
    pub(super) fn rows(ast: &'a Ast, rows: &'a [u32]) -> Self {
        Self {
            ast,
            source: ChildSource::Rows(rows.iter()),
        }
    }

    pub(super) fn fields(node: SyntaxNode<'a>, count: u8) -> Self {
        let fields = Fields {
            node: node.node_id(),
            unvisited: 0..count,
            rows: slice::Iter::default(),
        };
        Self {
            ast: node.ast,
            source: ChildSource::Fields(fields),
        }
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = SyntaxNode<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let handle = match &mut self.source {
            ChildSource::Empty => return None,
            ChildSource::Rows(words) => AstNodeId::from_word(*words.next()?),
            #[cfg(target_endian = "big")]
            ChildSource::InlineWords(words) => {
                AstNodeId::from_word(u32::from_le(*words.split_off_first()?))
            }
            ChildSource::Sequence { first, rows, last } => first
                .take()
                .or_else(|| rows.next().copied().map(AstNodeId::from_word))
                .or_else(|| last.take())?,
            ChildSource::Fields(fields) => fields.next(self.ast)?,
        };
        Some(self.ast.at(handle))
    }
}

impl FusedIterator for Children<'_> {}

/// A node and all its descendants, in preorder.
pub struct Descendants<'a> {
    ast: &'a Ast,
    next: Option<NonZeroU32>,
    pending: Pending<'a>,
}

impl<'a> Descendants<'a> {
    pub(super) fn new(root: SyntaxNode<'a>) -> Self {
        Self {
            ast: root.ast,
            next: Some(root.node_id().slot),
            pending: Pending::default(),
        }
    }

    /// Yields each remaining node in source order, skipping the children of nodes
    /// for which `predicate` returns `true`. The matching node itself is yielded.
    pub fn skip_subtrees<P>(self, predicate: P) -> impl FusedIterator<Item = SyntaxNode<'a>>
    where
        P: FnMut(SyntaxNode<'a>) -> bool,
    {
        SkipSubtrees {
            descendants: self,
            predicate,
        }
    }
}

impl<'a> Iterator for Descendants<'a> {
    type Item = SyntaxNode<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let node = self.ast.child(u64::from(self.next?.get()));
        self.next = NonZeroU32::new(self.ast.next_descendant(node, &mut self.pending));
        Some(node)
    }
}

impl FusedIterator for Descendants<'_> {}

/// A descendant iterator that can skip subtrees.
struct SkipSubtrees<'a, P> {
    descendants: Descendants<'a>,
    predicate: P,
}

impl<'a, P: FnMut(SyntaxNode<'a>) -> bool> Iterator for SkipSubtrees<'a, P> {
    type Item = SyntaxNode<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let ast = self.descendants.ast;
        let node = ast.child(u64::from(self.descendants.next?.get()));
        let pending = &mut self.descendants.pending;
        let next = if (self.predicate)(node) {
            pending.resume()
        } else {
            ast.next_descendant(node, pending)
        };
        self.descendants.next = NonZeroU32::new(next);
        Some(node)
    }
}

impl<'a, P: FnMut(SyntaxNode<'a>) -> bool> FusedIterator for SkipSubtrees<'a, P> {}

/// Statements in a subtree, in source order, with compound statements before their bodies.
pub(super) struct DescendantStatements<'a> {
    ast: &'a Ast,
    root: Option<Stmt<'a>>,
    pending: StatementLists<'a>,
}

impl<'a> DescendantStatements<'a> {
    pub(super) fn new(root: SyntaxNode<'a>) -> Self {
        let mut pending = StatementLists::default();
        let stmt = root.ast.enter_statement_node(root, &mut pending);
        Self {
            ast: root.ast,
            root: stmt,
            pending,
        }
    }

    #[inline]
    fn next_nested(&mut self) -> Option<Stmt<'a>> {
        while let Some(word) = self.pending.next_row() {
            let node = self.ast.child(u64::from(word));
            if let Some(stmt) = self.ast.enter_statement_node(node, &mut self.pending) {
                return Some(stmt);
            }
        }
        None
    }
}

impl<'a> Iterator for DescendantStatements<'a> {
    type Item = Stmt<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.root.take().or_else(|| self.next_nested())
    }

    #[inline]
    fn fold<B, F: FnMut(B, Self::Item) -> B>(mut self, init: B, mut f: F) -> B {
        let mut value = self.root.take().into_iter().fold(init, &mut f);
        while let Some(stmt) = self.next_nested() {
            value = f(value, stmt);
        }
        value
    }
}

impl FusedIterator for DescendantStatements<'_> {}

/// Suites, exception handlers, match cases and elif/else clauses are all lists.
#[derive(Default)]
pub(super) struct StatementLists<'a> {
    current: slice::Iter<'a, u32>,
    outer: SmallVec<[&'a [u32]; 4]>,
}

impl<'a> StatementLists<'a> {
    pub(super) fn push_list(&mut self, rows: &'a [u32]) {
        if rows.is_empty() {
            return;
        }
        if !self.current.as_slice().is_empty() {
            self.outer.push(self.current.as_slice());
        }
        self.current = rows.iter();
    }

    #[inline]
    fn next_row(&mut self) -> Option<u32> {
        if let Some(&word) = self.current.next() {
            return Some(word);
        }
        self.current = self.outer.pop()?.iter();
        self.current.next().copied()
    }
}

/// Fixed child fields need only a handle. A list stores its cursor at the depth
/// where its next sibling should be visited after nested fields finish.
pub(super) struct Pending<'a> {
    nodes: Vec<u32>,
    // Resume this slice once all scalar fields pushed after it have been consumed.
    // With no suspended list, the sentinel depth makes `resume` pop scalar fields directly.
    rows: slice::Iter<'a, u32>,
    depth: usize,
    outer: Vec<(usize, &'a [u32])>,
}

impl Default for Pending<'_> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            rows: slice::Iter::default(),
            depth: usize::MAX,
            outer: Vec::new(),
        }
    }
}

impl<'a> Pending<'a> {
    #[inline]
    pub(super) fn push_node(&mut self, node: u32) {
        self.nodes.push(node);
    }

    #[inline]
    fn suspend(&mut self) {
        if !self.rows.as_slice().is_empty() {
            self.outer
                .push((self.depth, mem::take(&mut self.rows).as_slice()));
        }
        self.depth = self.nodes.len();
    }

    #[inline]
    pub(super) fn descend_list(&mut self, words: &'a [u32]) -> u32 {
        let Some((&first, rest)) = words.split_first() else {
            return self.resume();
        };
        self.push_list(rest);
        first
    }

    /// Returns the next stored child ID, or the reserved slot zero at the end of the walk.
    #[inline]
    pub(super) fn resume(&mut self) -> u32 {
        if self.nodes.len() != self.depth {
            return self.nodes.pop().unwrap_or(0);
        }
        if let Some(&first) = self.rows.next() {
            return first;
        }
        self.resume_outer()
    }

    fn resume_outer(&mut self) -> u32 {
        let Some(&(depth, rows)) = self.outer.last() else {
            self.depth = usize::MAX;
            return self.nodes.pop().unwrap_or(0);
        };
        self.depth = depth;
        if self.nodes.len() != depth {
            return self.nodes.pop().unwrap_or(0);
        }
        self.outer.pop();
        self.rows = rows.iter();
        self.rows.next().copied().unwrap_or(0)
    }

    #[inline]
    pub(super) fn push_list(&mut self, words: &'a [u32]) {
        match words {
            [] => {}
            &[only] => self.nodes.push(only),
            words => {
                self.suspend();
                self.rows = words.iter();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use ruff_text_size::TextRange;

    use super::ChildSource;
    use crate::flat::{AstNode, BuildId, Builder, Emit, build};
    use crate::{AstNodeId, NodeKind, Operator};

    #[test]
    fn children_borrow_inline_operands() {
        let range = TextRange::default();
        let mut builder = Builder::default();
        let (left, ..) = build::ExprNoneLiteral { range }.emit(&mut builder);
        let (right, ..) = build::ExprNoneLiteral { range }.emit(&mut builder);
        let (binary, ..) = build::ExprBinOp {
            range,
            left: left.into(),
            op: Operator::Add,
            right: right.into(),
        }
        .emit(&mut builder);
        let tree = builder.finish_tree(binary);
        let binary: crate::ExprBinOp<'_> = tree.root();
        let id: AstNodeId<crate::kind::ExprBinOp> = binary.node_id();
        assert_eq!(tree.ast().view(id), Some(binary));
        assert_eq!(
            binary.syntax_node().cast::<crate::ExprBinOp>(),
            Some(binary)
        );
        assert_eq!(format!("{:?}", binary.syntax_node()), "ExprBinOp@0..0");
        assert_eq!(
            format!("{:#?}", binary.syntax_node()),
            format!("{binary:#?}")
        );
        let mut children = binary.children();
        let words = match &children.source {
            ChildSource::Rows(rows) => Some(rows.as_slice()),
            #[cfg(target_endian = "big")]
            ChildSource::InlineWords(words) => Some(*words),
            _ => None,
        };
        assert_eq!(
            words.map(<[u32]>::as_ptr),
            Some(ptr::from_ref(binary.syntax_node().row).cast())
        );
        assert_eq!(children.next(), Some(binary.left().syntax_node()));
        assert_eq!(children.next(), Some(binary.right().syntax_node()));
        assert!(children.next().is_none());
    }

    #[test]
    fn descendants_borrow_wide_suites() {
        let mut builder = Builder::default();
        let range = TextRange::default();
        let body = builder.map_list(0..1024, |builder, _| {
            BuildId::<crate::kind::Stmt>::from(builder.add(build::StmtPass { range }))
        });
        let root = builder.add(build::ModModule { range, body });
        let module = builder.get(root);
        let mut nodes = module.descendants();
        assert_eq!(
            nodes.next().map(AstNode::node_kind),
            Some(NodeKind::ModModule)
        );
        assert_eq!(
            nodes.next().map(AstNode::node_kind),
            Some(NodeKind::StmtPass)
        );
        let words = module.body().words();
        let pending = nodes.pending.rows.as_slice();
        assert!(pending.len() >= words.len() - 2);
        assert_eq!(
            pending.as_ptr(),
            words[words.len() - pending.len()..].as_ptr()
        );
        assert_eq!(nodes.pending.nodes.capacity(), 0);
        assert_eq!(nodes.pending.outer.capacity(), 0);
    }
}
