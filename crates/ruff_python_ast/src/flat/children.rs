use std::iter::FusedIterator;
use std::num::NonZeroU32;
use std::slice;

use arrayvec::ArrayVec;
use ruff_text_size::Ranged;
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
    One(Option<AstNodeId<crate::kind::Any>>),
    Two([Option<AstNodeId<crate::kind::Any>>; 2]),
    Rows(slice::Iter<'a, u32>),
    Sequence {
        first: Option<AstNodeId<crate::kind::Any>>,
        rows: slice::Iter<'a, u32>,
        last: Option<AstNodeId<crate::kind::Any>>,
    },
    Fields(Fields<'a>),
    Merged(MergedRows<'a>),
}

/// Positional and keyword children are stored in separate lists.
#[derive(Clone)]
struct MergedRows<'a> {
    left: &'a [u32],
    right: &'a [u32],
}

impl MergedRows<'_> {
    fn next(&mut self, ast: &Ast) -> Option<u32> {
        let list = match (self.left.first(), self.right.first()) {
            (Some(&left), Some(&right))
                if ast.child::<SyntaxNode<'_>>(u64::from(right)).start() < ast.child::<SyntaxNode<'_>>(u64::from(left)).start() =>
            {
                &mut self.right
            }
            (Some(_), _) => &mut self.left,
            _ => &mut self.right,
        };
        let (&word, rest) = list.split_first()?;
        *list = rest;
        Some(word)
    }
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

    pub(super) fn one(ast: &'a Ast, first: Option<AstNodeId<crate::kind::Any>>) -> Self {
        Self {
            ast,
            source: ChildSource::One(first),
        }
    }

    pub(super) fn two(
        ast: &'a Ast,
        first: Option<AstNodeId<crate::kind::Any>>,
        second: Option<AstNodeId<crate::kind::Any>>,
    ) -> Self {
        Self {
            ast,
            source: ChildSource::Two([first, second]),
        }
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

    pub(super) fn merged(ast: &'a Ast, left: &'a [u32], right: &'a [u32]) -> Self {
        if left.is_empty() {
            return Self::rows(ast, right);
        }
        if right.is_empty() {
            return Self::rows(ast, left);
        }
        Self {
            ast,
            source: ChildSource::Merged(MergedRows { left, right }),
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
            ChildSource::One(child) => child.take()?,
            ChildSource::Two([first, second]) => first.take().or_else(|| second.take())?,
            ChildSource::Sequence { first, rows, last } => first
                .take()
                .or_else(|| rows.next().copied().map(AstNodeId::from_word))
                .or_else(|| last.take())?,
            ChildSource::Fields(fields) => fields.next(self.ast)?,
            ChildSource::Merged(rows) => AstNodeId::from_word(rows.next(self.ast)?),
        };
        Some(self.ast.at(handle))
    }
}

impl FusedIterator for Children<'_> {}

/// A node and all its descendants, in preorder.
pub struct Descendants<'a> {
    ast: &'a Ast,
    next: Option<NonZeroU32>,
    pending: Pending,
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

/// Child IDs waiting to be visited, with the next sibling at the top.
#[derive(Default)]
pub(super) struct Pending {
    nodes: Vec<u32>,
}

impl Pending {
    #[inline]
    pub(super) fn push_node(&mut self, node: u32) {
        self.nodes.push(node);
    }

    #[inline]
    pub(super) fn push_list(&mut self, words: &[u32]) {
        self.nodes.extend(words.iter().rev().copied());
    }

    pub(super) fn descend_merged(&mut self, ast: &Ast, left: &[u32], right: &[u32]) -> u32 {
        if left.is_empty() {
            return self.descend_list(right);
        }
        if right.is_empty() {
            return self.descend_list(left);
        }
        let start = self.nodes.len();
        let mut rows = MergedRows { left, right };
        self.nodes.extend(std::iter::from_fn(|| rows.next(ast)));
        self.nodes[start..].reverse();
        self.resume()
    }

    #[inline]
    pub(super) fn descend_list(&mut self, words: &[u32]) -> u32 {
        let Some((&first, rest)) = words.split_first() else {
            return self.resume();
        };
        self.push_list(rest);
        first
    }

    #[inline]
    pub(super) fn resume(&mut self) -> u32 {
        self.nodes.pop().unwrap_or(0)
    }
}

impl<'a> crate::Parameters<'a> {
    pub(super) fn parameter_children(self) -> Children<'a> {
        if self.has_only_regular_args() {
            Children::rows(self.ast(), self.args().words())
        } else {
            Children::fields(self.syntax_node(), 7)
        }
    }

    pub(super) fn first_parameter(self, pending: &mut Pending) -> u32 {
        if self.has_only_regular_args() {
            return pending.descend_list(self.args().words());
        }
        if self.vararg().is_none() && self.kwarg().is_none() {
            pending.push_list(self.kwonlyargs().words());
            pending.push_list(self.args().words());
            return pending.descend_list(self.posonlyargs().words());
        }
        for field in self.child_fields().into_iter().rev() {
            match field {
                Field::One(Some(node)) => pending.push_node(node.slot.get()),
                Field::Rows(words) => pending.push_list(words),
                Field::One(None) => {}
            }
        }
        pending.resume()
    }

    pub(super) fn child_fields(self) -> ArrayVec<Field<'a>, 7> {
        // Each variadic can split one of the three ordinary lists into two slices.
        let mut fields = ArrayVec::new();
        let mut variadics = [self.vararg(), self.kwarg()];
        variadics.sort_by_key(|p| p.map_or(u32::MAX, |p| p.start().to_u32()));
        let mut variadics = variadics.into_iter().flatten().peekable();
        for mut ordinary in [self.posonlyargs(), self.args(), self.kwonlyargs()] {
            while let Some(variadic) = variadics.peek() {
                // Error recovery can put a variadic parameter before ordinary ones.
                let index = ordinary.partition_point(|p| p.start() <= variadic.start());
                let (before, after) = ordinary.split_at(index);
                if !before.is_empty() {
                    fields.push(Field::Rows(before.words()));
                }
                ordinary = after;
                if ordinary.is_empty() {
                    break;
                }
                fields.push(Field::One(Some(variadic.syntax_node().node_id())));
                variadics.next();
            }
            if !ordinary.is_empty() {
                fields.push(Field::Rows(ordinary.words()));
            }
        }
        for p in variadics {
            fields.push(Field::One(Some(p.syntax_node().node_id())));
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextRange;

    use crate::flat::{AstNode, BuildId, Builder, Emit, build};
    use crate::{AstNodeId, NodeKind, Operator};

    #[test]
    fn binary_children_follow_source_order() {
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
        assert_eq!(children.next(), Some(binary.left().syntax_node()));
        assert_eq!(children.next(), Some(binary.right().syntax_node()));
        assert!(children.next().is_none());
    }

    #[test]
    fn descendants_visit_wide_suites() {
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
        assert_eq!(nodes.count(), 1023);
    }
}
