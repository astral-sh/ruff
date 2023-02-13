use crate::green::NodeCacheNodeEntryMut;
use crate::{
    cow_mut::CowMut,
    green::{GreenElement, NodeCache},
    syntax::TriviaPiece,
    GreenNode, Language, NodeOrToken, ParsedChildren, SyntaxFactory, SyntaxKind, SyntaxNode,
};
use std::marker::PhantomData;

/// A checkpoint for maybe wrapping a node. See `GreenNodeBuilder::checkpoint` for details.
#[derive(Clone, Copy, Debug)]
pub struct Checkpoint(usize);

/// A builder for a syntax tree.
#[derive(Debug)]
pub struct TreeBuilder<'cache, L: Language, S: SyntaxFactory<Kind = L::Kind>> {
    cache: CowMut<'cache, NodeCache>,
    parents: Vec<(L::Kind, usize)>,
    children: Vec<(u64, GreenElement)>,
    ph: PhantomData<S>,
}

impl<L: Language, S: SyntaxFactory<Kind = L::Kind>> Default for TreeBuilder<'_, L, S> {
    fn default() -> Self {
        Self {
            cache: CowMut::default(),
            parents: Vec::default(),
            children: Vec::default(),
            ph: PhantomData,
        }
    }
}

impl<L: Language, S: SyntaxFactory<Kind = L::Kind>> TreeBuilder<'_, L, S> {
    /// Creates new builder.
    pub fn new() -> TreeBuilder<'static, L, S> {
        TreeBuilder::default()
    }

    /// Reusing `NodeCache` between different [TreeBuilder]`s saves memory.
    /// It allows to structurally share underlying trees.
    pub fn with_cache(cache: &mut NodeCache) -> TreeBuilder<'_, L, S> {
        TreeBuilder {
            cache: CowMut::Borrowed(cache),
            parents: Vec::new(),
            children: Vec::new(),
            ph: PhantomData,
        }
    }

    /// Method to quickly wrap a tree with a node.
    ///
    /// TreeBuilder::<RawLanguage>::wrap_with_node(RawSyntaxKind(0), |builder| {
    ///     builder.token(RawSyntaxKind(1), "let");
    /// });
    pub fn wrap_with_node<F>(kind: L::Kind, build: F) -> SyntaxNode<L>
    where
        F: Fn(&mut Self),
    {
        let mut builder = TreeBuilder::<L, S>::new();
        builder.start_node(kind);
        build(&mut builder);
        builder.finish_node();
        builder.finish()
    }

    /// Adds new token to the current branch.
    #[inline]
    pub fn token(&mut self, kind: L::Kind, text: &str) -> &mut Self {
        let (hash, token) = self.cache.token(kind.to_raw(), text);
        self.children.push((hash, token.into()));
        self
    }

    /// Adds new token to the current branch.
    #[inline]
    pub fn token_with_trivia(
        &mut self,
        kind: L::Kind,
        text: &str,
        leading: &[TriviaPiece],
        trailing: &[TriviaPiece],
    ) {
        let (hash, token) = self
            .cache
            .token_with_trivia(kind.to_raw(), text, leading, trailing);
        self.children.push((hash, token.into()));
    }

    /// Start new node and make it current.
    #[inline]
    pub fn start_node(&mut self, kind: L::Kind) -> &mut Self {
        let len = self.children.len();
        self.parents.push((kind, len));
        self
    }

    /// Finish current branch and restore previous
    /// branch as current.
    #[inline]
    pub fn finish_node(&mut self) -> &mut Self {
        let (kind, first_child) = self.parents.pop().unwrap();
        let raw_kind = kind.to_raw();

        let slots = &self.children[first_child..];
        let node_entry = self.cache.node(raw_kind, slots);

        let mut build_node = || {
            let children = ParsedChildren::new(&mut self.children, first_child);

            S::make_syntax(kind, children).into_green()
        };

        let (hash, node) = match node_entry {
            NodeCacheNodeEntryMut::NoCache(hash) => (hash, build_node()),
            NodeCacheNodeEntryMut::Vacant(entry) => {
                let node = build_node();

                let hash = entry.cache(node.clone());
                (hash, node)
            }
            NodeCacheNodeEntryMut::Cached(cached) => {
                self.children.truncate(first_child);
                (cached.hash(), cached.node().clone())
            }
        };

        self.children.push((hash, node.into()));
        self
    }

    /// Prepare for maybe wrapping the next node.
    /// The way wrapping works is that you first of all get a checkpoint,
    /// then you place all tokens you want to wrap, and then *maybe* call
    /// `start_node_at`.
    /// Example:
    /// ```rust
    /// # use ruff_rowan::raw_language::{RawLanguage, RawLanguageKind, RawSyntaxTreeBuilder};
    /// # const PLUS: RawLanguageKind = RawLanguageKind::PLUS_TOKEN;
    /// # const OPERATION: RawLanguageKind = RawLanguageKind::ROOT;
    /// # struct Parser;
    /// # impl Parser {
    /// #     fn peek(&self) -> Option<RawLanguageKind> { None }
    /// #     fn parse_expr(&mut self) {}
    /// # }
    /// # let mut builder = RawSyntaxTreeBuilder::new();
    /// # let mut parser = Parser;
    /// let checkpoint = builder.checkpoint();
    /// parser.parse_expr();
    /// if parser.peek() == Some(PLUS) {
    ///     // 1 + 2 = Add(1, 2)
    ///     builder.start_node_at(checkpoint, OPERATION);
    ///     parser.parse_expr();
    ///     builder.finish_node();
    /// }
    /// ```
    #[inline]
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.children.len())
    }

    /// Wrap the previous branch marked by `checkpoint` in a new branch and
    /// make it current.
    #[inline]
    pub fn start_node_at(&mut self, checkpoint: Checkpoint, kind: L::Kind) {
        let Checkpoint(checkpoint) = checkpoint;
        assert!(
            checkpoint <= self.children.len(),
            "checkpoint no longer valid, was finish_node called early?"
        );

        if let Some(&(_, first_child)) = self.parents.last() {
            assert!(
                checkpoint >= first_child,
                "checkpoint no longer valid, was an unmatched start_node_at called?"
            );
        }

        self.parents.push((kind, checkpoint));
    }

    /// Complete tree building. Make sure that
    /// `start_node_at` and `finish_node` calls
    /// are paired!
    #[inline]
    #[must_use]
    pub fn finish(self) -> SyntaxNode<L> {
        SyntaxNode::new_root(self.finish_green())
    }

    // For tests
    #[must_use]
    pub(crate) fn finish_green(mut self) -> GreenNode {
        assert_eq!(self.children.len(), 1);
        match self.children.pop().unwrap().1 {
            NodeOrToken::Node(node) => node,
            _ => panic!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::green::GreenElementRef;
    use crate::raw_language::{RawLanguageKind, RawSyntaxTreeBuilder};
    use crate::{GreenNodeData, GreenTokenData, NodeOrToken};

    // Builds a "Condition" like structure where the closing ) is missing
    fn build_condition_with_missing_closing_parenthesis(builder: &mut RawSyntaxTreeBuilder) {
        builder.start_node(RawLanguageKind::CONDITION);

        builder.token(RawLanguageKind::L_PAREN_TOKEN, "(");

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::STRING_TOKEN, "a");
        builder.finish_node();

        // missing )

        builder.finish_node();
    }

    #[test]
    fn caches_identical_nodes_with_empty_slots() {
        let mut builder = RawSyntaxTreeBuilder::new();

        builder.start_node(RawLanguageKind::ROOT); // Root
        build_condition_with_missing_closing_parenthesis(&mut builder);
        build_condition_with_missing_closing_parenthesis(&mut builder);
        builder.finish_node();

        let root = builder.finish_green();

        let first = root.children().next().unwrap();
        let last = root.children().last().unwrap();

        assert_eq!(first.element(), last.element());
        assert_same_elements(first.element(), last.element());
    }

    #[test]
    fn doesnt_cache_node_if_empty_slots_differ() {
        let mut builder = RawSyntaxTreeBuilder::new();

        builder.start_node(RawLanguageKind::ROOT); // Root
        build_condition_with_missing_closing_parenthesis(&mut builder); // misses the ')'

        // Create a well formed condition
        builder.start_node(RawLanguageKind::CONDITION);

        builder.token(RawLanguageKind::L_PAREN_TOKEN, "(");

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::STRING_TOKEN, "a");
        builder.finish_node();

        // missing )
        builder.token(RawLanguageKind::R_PAREN_TOKEN, ")");

        builder.finish_node();

        // finish root
        builder.finish_node();

        let root = builder.finish_green();
        let first_condition = root.children().next().unwrap();
        let last_condition = root.children().last().unwrap();

        assert_ne!(first_condition.element(), last_condition.element());
    }

    fn assert_same_elements(left: GreenElementRef<'_>, right: GreenElementRef<'_>) {
        fn element_id(element: GreenElementRef<'_>) -> *const () {
            match element {
                NodeOrToken::Node(node) => node as *const GreenNodeData as *const (),
                NodeOrToken::Token(token) => token as *const GreenTokenData as *const (),
            }
        }

        assert_eq!(element_id(left), element_id(right),);
    }
}
