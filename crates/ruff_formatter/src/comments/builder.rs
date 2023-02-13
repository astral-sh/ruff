use super::{
    map::CommentsMap, CommentPlacement, CommentStyle, CommentTextPosition, DecoratedComment,
    SourceComment, TransformSourceMap,
};
use crate::source_map::{DeletedRangeEntry, DeletedRanges};
use crate::{TextRange, TextSize};
use ruff_rowan::syntax::SyntaxElementKey;
use ruff_rowan::{
    Direction, Language, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, WalkEvent,
};
use rustc_hash::FxHashSet;

/// Extracts all comments from a syntax tree.
pub(super) struct CommentsBuilderVisitor<'a, Style: CommentStyle> {
    builder: CommentsBuilder<Style::Language>,
    style: &'a Style,
    parentheses: SourceParentheses<'a>,

    // State
    pending_comments: Vec<DecoratedComment<Style::Language>>,
    preceding_node: Option<SyntaxNode<Style::Language>>,
    following_node_index: Option<usize>,
    parents: Vec<SyntaxNode<Style::Language>>,
    last_token: Option<SyntaxToken<Style::Language>>,
}

impl<'a, Style> CommentsBuilderVisitor<'a, Style>
where
    Style: CommentStyle,
{
    pub(super) fn new(style: &'a Style, source_map: Option<&'a TransformSourceMap>) -> Self {
        Self {
            style,
            builder: Default::default(),
            parentheses: SourceParentheses::from_source_map(source_map),

            pending_comments: Default::default(),
            preceding_node: Default::default(),
            following_node_index: Default::default(),
            parents: Default::default(),
            last_token: Default::default(),
        }
    }

    pub(super) fn visit(
        mut self,
        root: &SyntaxNode<Style::Language>,
    ) -> (
        CommentsMap<SyntaxElementKey, SourceComment<Style::Language>>,
        FxHashSet<SyntaxElementKey>,
    ) {
        for event in root.preorder_with_tokens(Direction::Next) {
            match event {
                WalkEvent::Enter(SyntaxElement::Node(node)) => {
                    self.visit_node(WalkEvent::Enter(node))
                }

                WalkEvent::Leave(SyntaxElement::Node(node)) => {
                    self.visit_node(WalkEvent::Leave(node))
                }

                WalkEvent::Enter(SyntaxElement::Token(token)) => self.visit_token(token),
                WalkEvent::Leave(SyntaxElement::Token(_)) => {
                    // Handled as part of enter
                }
            }
        }

        assert!(
            self.parents.is_empty(),
            "Expected all enclosing nodes to have been processed but contains {:#?}",
            self.parents
        );

        // Process any comments attached to the last token.
        // Important for range formatting where it isn't guaranteed that the
        // last token is an EOF token.
        if let Some(last_token) = self.last_token.take() {
            self.parents.push(root.clone());
            let (comments_start, lines_before, position, trailing_end) =
                self.visit_trailing_comments(last_token, None);
            Self::update_comments(
                &mut self.pending_comments[comments_start..],
                position,
                lines_before,
                trailing_end,
            );
        }

        self.flush_comments(None);

        self.builder.finish()
    }

    fn visit_node(&mut self, event: WalkEvent<SyntaxNode<Style::Language>>) {
        match event {
            WalkEvent::Enter(node) => {
                // Lists cannot have comments attached. They either belong to the entire parent or to
                // the first child. So we ignore lists all together
                if node.kind().is_list() {
                    return;
                }

                let is_root = matches!(self.following_node_index, Some(0));

                // Associate comments with the most outer node
                // Set following here because it is the "following node" of the next token's leading trivia.
                if self.following_node_index.is_none() || is_root {
                    // Flush in case the node doesn't have any tokens.
                    self.flush_comments(Some(&node));
                    self.following_node_index = Some(self.parents.len());
                }

                self.parents.push(node);
            }

            WalkEvent::Leave(node) => {
                if node.kind().is_list() {
                    return;
                }

                self.parents.pop().unwrap();

                // We're passed this node, flush any pending comments for its children
                self.following_node_index = None;
                self.flush_comments(None);

                // We're passed this node, so it must precede the sibling that comes next.
                self.preceding_node = Some(node);
            }
        }
    }

    fn visit_token(&mut self, token: SyntaxToken<Style::Language>) {
        // Process the trailing trivia of the last token
        let (comments_start, mut lines_before, mut position, mut trailing_end) =
            if let Some(last_token) = self.last_token.take() {
                self.visit_trailing_comments(last_token, Some(&token))
            } else {
                (
                    self.pending_comments.len(),
                    0,
                    CommentTextPosition::SameLine,
                    None,
                )
            };

        // Process the leading trivia of the current token. the trailing trivia is handled as part of the next token
        for leading in token.leading_trivia().pieces() {
            if leading.is_newline() {
                lines_before += 1;
                // All comments following from here are own line comments
                position = CommentTextPosition::OwnLine;

                if trailing_end.is_none() {
                    trailing_end = Some(self.pending_comments.len());
                }
            } else if leading.is_skipped() {
                self.builder.mark_has_skipped(&token);

                lines_before = 0;
                break;
            } else if let Some(comment) = leading.as_comments() {
                let kind = Style::get_comment_kind(&comment);

                self.queue_comment(DecoratedComment {
                    enclosing: self.enclosing_node().clone(),
                    preceding: self.preceding_node.clone(),
                    following: None,
                    following_token: Some(token.clone()),
                    lines_before,
                    lines_after: 0,
                    text_position: position,
                    kind,
                    comment,
                });

                lines_before = 0;
            }
        }

        self.last_token = Some(token);

        Self::update_comments(
            &mut self.pending_comments[comments_start..],
            position,
            lines_before,
            trailing_end,
        );

        // Set following node to `None` because it now becomes the enclosing node.
        if let Some(following_node) = self.following_node() {
            self.flush_comments(Some(&following_node.clone()));
            self.following_node_index = None;

            // The following node is only set after entering a node
            // That means, following node is only set for the first token of a node.
            // Unset preceding node if this is the first token because the preceding node belongs to the parent.
            self.preceding_node = None;
        }
    }

    fn enclosing_node(&self) -> &SyntaxNode<Style::Language> {
        let element = match self.following_node_index {
            None => self.parents.last(),
            Some(index) if index == 0 => Some(&self.parents[0]),
            Some(index) => Some(&self.parents[index - 1]),
        };

        element.expect("Expected enclosing nodes to at least contain the root node.")
    }

    fn following_node(&self) -> Option<&SyntaxNode<Style::Language>> {
        self.following_node_index.map(|index| {
            self.parents
                .get(index)
                .expect("Expected following node index to point to a valid parent node")
        })
    }

    fn queue_comment(&mut self, comment: DecoratedComment<Style::Language>) {
        self.pending_comments.push(comment);
    }

    fn update_comments(
        comments: &mut [DecoratedComment<Style::Language>],
        position: CommentTextPosition,
        lines_before: u32,
        trailing_end: Option<usize>,
    ) {
        let trailing_end = trailing_end.unwrap_or(comments.len());
        let mut comments = comments.iter_mut().enumerate().peekable();

        // Update the lines after of all comments as well as the positioning of end of line comments.
        while let Some((index, comment)) = comments.next() {
            // Update the position of all trailing comments to be end of line as we've seen a line break since.
            if index < trailing_end && position.is_own_line() {
                comment.text_position = CommentTextPosition::EndOfLine;
            }

            comment.lines_after = comments
                .peek()
                .map_or(lines_before, |(_, next)| next.lines_before);
        }
    }

    fn flush_comments(&mut self, following: Option<&SyntaxNode<Style::Language>>) {
        for mut comment in self.pending_comments.drain(..) {
            comment.following = following.cloned();

            let placement = self.style.place_comment(comment);
            self.builder.add_comment(placement);
        }
    }

    fn visit_trailing_comments(
        &mut self,
        token: SyntaxToken<Style::Language>,
        following_token: Option<&SyntaxToken<Style::Language>>,
    ) -> (usize, u32, CommentTextPosition, Option<usize>) {
        let mut comments_start = 0;

        // The index of the last trailing comment in `pending_comments`.
        let mut trailing_end: Option<usize> = None;

        // Number of lines before the next comment, token, or skipped token trivia
        let mut lines_before = 0;

        // Trailing comments are all `SameLine` comments EXCEPT if any is followed by a line break,
        // a leading comment (that always have line breaks), or there's a line break before the token.
        let mut position = CommentTextPosition::SameLine;

        // Process the trailing trivia of the last token
        for piece in token.trailing_trivia().pieces() {
            if piece.is_newline() {
                lines_before += 1;
                // All comments following from here are own line comments
                position = CommentTextPosition::OwnLine;

                if trailing_end.is_none() {
                    trailing_end = Some(self.pending_comments.len());
                }
            } else if let Some(comment) = piece.as_comments() {
                self.queue_comment(DecoratedComment {
                    enclosing: self.enclosing_node().clone(),
                    preceding: self.preceding_node.clone(),
                    following: None,
                    following_token: following_token.cloned(),
                    lines_before,
                    lines_after: 0, // Will be initialized after
                    text_position: position,
                    kind: Style::get_comment_kind(&comment),
                    comment,
                });

                lines_before = 0;
            }

            if let Some(parens_source_range) = self
                .parentheses
                .r_paren_source_range(piece.text_range().end())
            {
                self.flush_before_r_paren_comments(
                    parens_source_range,
                    &token,
                    position,
                    lines_before,
                    comments_start,
                    trailing_end,
                );

                lines_before = 0;
                position = CommentTextPosition::SameLine;
                comments_start = 0;
                trailing_end = None;
            }
        }

        (comments_start, lines_before, position, trailing_end)
    }

    /// Processes comments appearing right before a `)` of a parenthesized expressions.
    #[cold]
    fn flush_before_r_paren_comments(
        &mut self,
        parens_source_range: TextRange,
        last_token: &SyntaxToken<Style::Language>,
        position: CommentTextPosition,
        lines_before: u32,
        start: usize,
        trailing_end: Option<usize>,
    ) {
        let enclosing = self.enclosing_node().clone();

        let comments = &mut self.pending_comments[start..];
        let trailing_end = trailing_end.unwrap_or(comments.len());

        let mut comments = comments.iter_mut().enumerate().peekable();

        let parenthesized_node = self
            .parentheses
            .outer_most_parenthesized_node(last_token, parens_source_range);

        let preceding = parenthesized_node;

        // Using the `enclosing` as default but it's mainly to satisfy Rust. The only case where it is used
        // is if someone formats a Parenthesized expression as the root. Something we explicitly disallow
        // in ruff_js_formatter
        let enclosing = preceding.parent().unwrap_or(enclosing);

        // Update the lines after of all comments as well as the positioning of end of line comments.
        while let Some((index, comment)) = comments.next() {
            // Update the position of all trailing comments to be end of line as we've seen a line break since.
            if index < trailing_end && position.is_own_line() {
                comment.text_position = CommentTextPosition::EndOfLine;
            }

            comment.preceding = Some(preceding.clone());
            comment.enclosing = enclosing.clone();
            comment.lines_after = comments
                .peek()
                .map_or(lines_before, |(_, next)| next.lines_before);
        }

        self.flush_comments(None);
    }
}

struct CommentsBuilder<L: Language> {
    comments: CommentsMap<SyntaxElementKey, SourceComment<L>>,
    skipped: FxHashSet<SyntaxElementKey>,
}

impl<L: Language> CommentsBuilder<L> {
    fn add_comment(&mut self, placement: CommentPlacement<L>) {
        match placement {
            CommentPlacement::Leading { node, comment } => {
                self.push_leading_comment(&node, comment);
            }
            CommentPlacement::Trailing { node, comment } => {
                self.push_trailing_comment(&node, comment);
            }
            CommentPlacement::Dangling { node, comment } => {
                self.push_dangling_comment(&node, comment)
            }
            CommentPlacement::Default(mut comment) => {
                match comment.text_position {
                    CommentTextPosition::EndOfLine => {
                        match (comment.take_preceding_node(), comment.take_following_node()) {
                            (Some(preceding), Some(_)) => {
                                // Attach comments with both preceding and following node to the preceding
                                // because there's a line break separating it from the following node.
                                // ```javascript
                                // a; // comment
                                // b
                                // ```
                                self.push_trailing_comment(&preceding, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(&preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(&following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(
                                    &comment.enclosing_node().clone(),
                                    comment,
                                );
                            }
                        }
                    }
                    CommentTextPosition::OwnLine => {
                        match (comment.take_preceding_node(), comment.take_following_node()) {
                            // Following always wins for a leading comment
                            // ```javascript
                            // a;
                            // // comment
                            // b
                            // ```
                            // attach the comment to the `b` expression statement
                            (_, Some(following)) => {
                                self.push_leading_comment(&following, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(&preceding, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(
                                    &comment.enclosing_node().clone(),
                                    comment,
                                );
                            }
                        }
                    }
                    CommentTextPosition::SameLine => {
                        match (comment.take_preceding_node(), comment.take_following_node()) {
                            (Some(preceding), Some(following)) => {
                                // Only make it a trailing comment if it directly follows the preceding node but not if it is separated
                                // by one or more tokens
                                // ```javascript
                                // a /* comment */ b;   //  Comment is a trailing comment
                                // a, /* comment */ b;  // Comment should be a leading comment
                                // ```
                                if preceding.text_range().end()
                                    == comment.piece().as_piece().token().text_range().end()
                                {
                                    self.push_trailing_comment(&preceding, comment);
                                } else {
                                    self.push_leading_comment(&following, comment);
                                }
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(&preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(&following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(
                                    &comment.enclosing_node().clone(),
                                    comment,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn mark_has_skipped(&mut self, token: &SyntaxToken<L>) {
        self.skipped.insert(token.key());
    }

    fn push_leading_comment(&mut self, node: &SyntaxNode<L>, comment: impl Into<SourceComment<L>>) {
        self.comments.push_leading(node.key(), comment.into());
    }

    fn push_dangling_comment(
        &mut self,
        node: &SyntaxNode<L>,
        comment: impl Into<SourceComment<L>>,
    ) {
        self.comments.push_dangling(node.key(), comment.into());
    }

    fn push_trailing_comment(
        &mut self,
        node: &SyntaxNode<L>,
        comment: impl Into<SourceComment<L>>,
    ) {
        self.comments.push_trailing(node.key(), comment.into());
    }

    fn finish(
        self,
    ) -> (
        CommentsMap<SyntaxElementKey, SourceComment<L>>,
        FxHashSet<SyntaxElementKey>,
    ) {
        (self.comments, self.skipped)
    }
}

impl<L: Language> Default for CommentsBuilder<L> {
    fn default() -> Self {
        Self {
            comments: CommentsMap::new(),
            skipped: FxHashSet::default(),
        }
    }
}

enum SourceParentheses<'a> {
    Empty,
    SourceMap {
        map: &'a TransformSourceMap,
        next: Option<DeletedRangeEntry<'a>>,
        tail: DeletedRanges<'a>,
    },
}

impl<'a> SourceParentheses<'a> {
    fn from_source_map(source_map: Option<&'a TransformSourceMap>) -> Self {
        match source_map {
            None => Self::Empty,
            Some(source_map) => {
                let mut deleted = source_map.deleted_ranges();
                SourceParentheses::SourceMap {
                    map: source_map,
                    next: deleted.next(),
                    tail: deleted,
                }
            }
        }
    }

    /// Returns the range of `node` including its parentheses if any. Otherwise returns the range as is
    fn parenthesized_range<L: Language>(&self, node: &SyntaxNode<L>) -> TextRange {
        match self {
            SourceParentheses::Empty => node.text_trimmed_range(),
            SourceParentheses::SourceMap { map, .. } => map.trimmed_source_range(node),
        }
    }

    /// Tests if the next offset is at a position where the original source document used to have an `)`.
    ///
    /// Must be called with offsets in increasing order.
    ///
    /// Returns the source range of the `)` if there's any `)` in the deleted range at this offset. Returns `None` otherwise

    fn r_paren_source_range(&mut self, offset: TextSize) -> Option<TextRange> {
        match self {
            SourceParentheses::Empty => None,
            SourceParentheses::SourceMap { next, tail, .. } => {
                while let Some(range) = next {
                    #[allow(clippy::comparison_chain)]
                    if range.transformed == offset {
                        // A deleted range can contain multiple tokens. See if there's any `)` in the deleted
                        // range and compute its source range.
                        return range.text.find(')').map(|r_paren_position| {
                            let start = range.source + TextSize::from(r_paren_position as u32);
                            TextRange::at(start, TextSize::from(1))
                        });
                    } else if range.transformed > offset {
                        return None;
                    } else {
                        *next = tail.next();
                    }
                }

                None
            }
        }
    }

    /// Searches the outer most node that still is inside of the parentheses specified by the `parentheses_source_range`.
    fn outer_most_parenthesized_node<L: Language>(
        &self,
        token: &SyntaxToken<L>,
        parentheses_source_range: TextRange,
    ) -> SyntaxNode<L> {
        match self {
            SourceParentheses::Empty => token.parent().unwrap(),
            SourceParentheses::SourceMap { map, .. } => {
                debug_assert_eq!(&map.text()[parentheses_source_range], ")");

                // How this works: We search the outer most node that, in the source document ends right after the `)`.
                // The issue is, it is possible that multiple nodes end right after the `)`
                //
                // ```javascript
                // !(
                //     a
                //     /* comment */
                //  )
                // ```
                // The issue is, that in the transformed document, the `ReferenceIdentifier`, `IdentifierExpression`, `UnaryExpression`, and `ExpressionStatement`
                // all end at the end position of `)`.
                // However, not all the nodes start at the same position. That's why this code also tracks the start.
                // We first find the closest node that directly ends at the position of the right paren. We then continue
                // upwards to find the most outer node that starts at the same position as that node. (In this case,
                // `ReferenceIdentifier` -> `IdentifierExpression`.
                let mut start_offset = None;
                let r_paren_source_end = parentheses_source_range.end();

                let ancestors = token.ancestors().take_while(|node| {
                    let source_range = self.parenthesized_range(node);

                    if let Some(start) = start_offset {
                        TextRange::new(start, r_paren_source_end).contains_range(source_range)
                    }
                    // Greater than to guarantee that we always return at least one node AND
                    // handle the case where a node is wrapped in multiple parentheses.
                    // Take the first node that fully encloses the parentheses
                    else if source_range.end() >= r_paren_source_end {
                        start_offset = Some(source_range.start());
                        true
                    } else {
                        source_range.end() < r_paren_source_end
                    }
                });

                // SAFETY:
                // * The builder starts with a node which guarantees that every token has a parent node.
                // * The above `take_while` guarantees to return `true` for the parent of the token.
                // Thus, there's always at least one node
                ancestors.last().unwrap()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CommentsBuilderVisitor;
    use crate::comments::{
        CommentKind, CommentPlacement, CommentStyle, CommentTextPosition, CommentsMap,
        DecoratedComment, SourceComment,
    };
    use crate::{TextSize, TransformSourceMap, TransformSourceMapBuilder};
    use ruff_js_parser::parse_module;
    use ruff_js_syntax::{
        JsIdentifierExpression, JsLanguage, JsParameters, JsParenthesizedExpression,
        JsPropertyObjectMember, JsReferenceIdentifier, JsShorthandPropertyObjectMember,
        JsSyntaxKind, JsSyntaxNode, JsUnaryExpression,
    };
    use ruff_rowan::syntax::SyntaxElementKey;
    use ruff_rowan::{AstNode, BatchMutation, SyntaxNode, SyntaxTriviaPieceComments, TextRange};
    use std::cell::RefCell;

    #[test]
    fn leading_comment() {
        let (root, decorated, comments) = extract_comments(
            r#"const foo = {
  a: 'a',
  /* comment for this line */
  b
};"#,
        );

        assert_eq!(decorated.len(), 1);

        let comment = decorated.last().unwrap();

        assert_eq!(comment.text_position(), CommentTextPosition::OwnLine);
        assert_eq!(comment.lines_before(), 1);
        assert_eq!(comment.lines_after(), 1);
        assert_eq!(
            comment
                .preceding_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("a: 'a'")
        );
        assert_eq!(
            comment
                .following_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            comment.enclosing_node().kind(),
            JsSyntaxKind::JS_OBJECT_EXPRESSION
        );

        let b = root
            .descendants()
            .find_map(JsShorthandPropertyObjectMember::cast)
            .unwrap();

        assert!(!comments.leading(&b.syntax().key()).is_empty());
    }

    #[test]
    fn trailing_comment() {
        let (root, decorated, comments) = extract_comments(
            r#"const foo = {
  a: 'a' /* comment for this line */,
  b
};"#,
        );

        assert_eq!(decorated.len(), 1);

        let comment = decorated.last().unwrap();

        assert_eq!(comment.text_position(), CommentTextPosition::SameLine);
        assert_eq!(comment.lines_after(), 0);
        assert_eq!(comment.lines_before(), 0);
        assert_eq!(
            comment
                .preceding_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("a: 'a'")
        );
        assert_eq!(
            comment
                .following_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            comment.enclosing_node().kind(),
            JsSyntaxKind::JS_OBJECT_EXPRESSION
        );

        let a = root
            .descendants()
            .find_map(JsPropertyObjectMember::cast)
            .unwrap();

        assert!(!comments.trailing(&a.syntax().key()).is_empty());
    }

    #[test]
    fn end_of_line_comment() {
        let (root, decorated, comments) = extract_comments(
            r#"const foo = {
  a: 'a', /* comment for this line */
  b
};"#,
        );

        assert_eq!(decorated.len(), 1);

        let comment = decorated.last().unwrap();

        assert_eq!(comment.text_position(), CommentTextPosition::EndOfLine);
        assert_eq!(comment.lines_before(), 0);
        assert_eq!(comment.lines_after(), 1);
        assert_eq!(
            comment
                .preceding_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("a: 'a'")
        );
        assert_eq!(
            comment
                .following_node()
                .map(|node| node.text_trimmed().to_string())
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            comment.enclosing_node().kind(),
            JsSyntaxKind::JS_OBJECT_EXPRESSION
        );

        let a = root
            .descendants()
            .find_map(JsPropertyObjectMember::cast)
            .unwrap();

        assert!(!comments.trailing(&a.syntax().key()).is_empty());
    }

    #[test]
    fn dangling_arrow() {
        let (root, decorated_comments, comments) = extract_comments("(/* comment */)  => true");

        assert_eq!(decorated_comments.len(), 1);

        let decorated = &decorated_comments[0];
        assert_eq!(decorated.text_position(), CommentTextPosition::SameLine);
        assert_eq!(decorated.lines_before(), 0);
        assert_eq!(decorated.lines_after(), 0);
        assert_eq!(decorated.preceding_node(), None);
        assert_eq!(decorated.following_node(), None);
        assert_eq!(
            decorated.enclosing_node().kind(),
            JsSyntaxKind::JS_PARAMETERS
        );

        let parameters = root.descendants().find_map(JsParameters::cast).unwrap();
        assert!(!comments.dangling(&parameters.syntax().key()).is_empty());
    }

    #[test]
    fn dangling_comments() {
        let (root, decorated_comments, comments) = extract_comments(
            r#"
            function (/* test */) {}
            "#,
        );

        assert_eq!(decorated_comments.len(), 1);

        let decorated = &decorated_comments[0];
        assert_eq!(decorated.text_position(), CommentTextPosition::SameLine);
        assert_eq!(decorated.lines_before(), 0);
        assert_eq!(decorated.lines_after(), 0);
        assert_eq!(decorated.preceding_node(), None);
        assert_eq!(decorated.following_node(), None);
        assert_eq!(
            decorated.enclosing_node().kind(),
            JsSyntaxKind::JS_PARAMETERS
        );

        let parameters = root.descendants().find_map(JsParameters::cast).unwrap();
        assert!(!comments.dangling(&parameters.syntax().key()).is_empty());
    }

    #[test]
    fn r_paren() {
        let source = r#"!(
    a
    /* comment */
)
/* comment */
b;"#;

        let mut source_map_builder = TransformSourceMapBuilder::with_source(source.to_string());
        let l_paren_range = TextRange::new(TextSize::from(1), TextSize::from(2));
        let r_paren_range = TextRange::new(TextSize::from(27), TextSize::from(28));

        assert_eq!(&source[l_paren_range], "(");
        assert_eq!(&source[r_paren_range], ")");

        source_map_builder.add_deleted_range(l_paren_range);
        source_map_builder.add_deleted_range(r_paren_range);
        source_map_builder.extend_trimmed_node_range(
            TextRange::new(TextSize::from(7), TextSize::from(8)),
            TextRange::new(l_paren_range.start(), r_paren_range.end()),
        );

        let source_map = source_map_builder.finish();

        let root = parse_module(source).syntax();

        // A lot of code that simply removes the parenthesized expression and moves the parens
        // trivia to the identifiers leading / trailing trivia.
        let parenthesized = root
            .descendants()
            .find_map(JsParenthesizedExpression::cast)
            .unwrap();

        let reference_identifier = root
            .descendants()
            .find_map(JsReferenceIdentifier::cast)
            .unwrap();

        let mut mutation = BatchMutation::new(root);

        let identifier_expression =
            JsIdentifierExpression::cast(parenthesized.expression().unwrap().into_syntax())
                .unwrap();
        let l_paren = parenthesized.l_paren_token().unwrap();
        let r_paren = parenthesized.r_paren_token().unwrap();

        let identifier_token = reference_identifier.value_token().unwrap();
        let new_identifier_token = identifier_token
            .with_leading_trivia_pieces(
                l_paren
                    .leading_trivia()
                    .pieces()
                    .chain(l_paren.trailing_trivia().pieces())
                    .chain(identifier_token.leading_trivia().pieces())
                    .collect::<Vec<_>>(),
            )
            .with_trailing_trivia_pieces(
                identifier_token
                    .trailing_trivia()
                    .pieces()
                    .chain(r_paren.leading_trivia().pieces())
                    .chain(r_paren.trailing_trivia().pieces())
                    .collect::<Vec<_>>(),
            );

        let new_reference_identifier = reference_identifier.with_value_token(new_identifier_token);

        let new_identifier_expression = identifier_expression.with_name(new_reference_identifier);

        mutation.replace_element_discard_trivia(
            parenthesized.into_syntax().into(),
            new_identifier_expression.into_syntax().into(),
        );

        let transformed = mutation.commit();

        let style = TestCommentStyle::default();
        let comments_builder = CommentsBuilderVisitor::new(&style, Some(&source_map));
        let (comments, _) = comments_builder.visit(&transformed);

        let decorated_comments = style.finish();

        assert_eq!(decorated_comments.len(), 2);

        let argument_trailing = &decorated_comments[0];
        assert_eq!(
            argument_trailing.text_position(),
            CommentTextPosition::OwnLine
        );
        assert_eq!(argument_trailing.lines_before(), 1);
        assert_eq!(argument_trailing.lines_after(), 1);
        assert_eq!(
            argument_trailing
                .preceding_node()
                .map(|preceding| preceding.kind()),
            Some(JsSyntaxKind::JS_IDENTIFIER_EXPRESSION)
        );
        assert_eq!(argument_trailing.following_node(), None);
        assert_eq!(
            argument_trailing.enclosing_node().kind(),
            JsSyntaxKind::JS_UNARY_EXPRESSION
        );

        let identifier_leading = &decorated_comments[1];
        assert_eq!(
            identifier_leading.text_position(),
            CommentTextPosition::OwnLine
        );
        assert_eq!(identifier_leading.lines_before(), 1);
        assert_eq!(identifier_leading.lines_after(), 1);
        assert_eq!(
            identifier_leading.preceding_node().map(SyntaxNode::kind),
            Some(JsSyntaxKind::JS_EXPRESSION_STATEMENT)
        );
        assert_eq!(
            identifier_leading.following_node().map(SyntaxNode::kind),
            Some(JsSyntaxKind::JS_EXPRESSION_STATEMENT)
        );
        assert_eq!(
            identifier_leading.enclosing_node().kind(),
            JsSyntaxKind::JS_MODULE
        );

        let unary = transformed
            .descendants()
            .find_map(JsUnaryExpression::cast)
            .unwrap();
        assert!(!comments
            .trailing(&unary.argument().unwrap().syntax().key())
            .is_empty());
    }

    #[test]
    fn comment_only_program() {
        let (root, decorated, comments) = extract_comments(
            r#"/* test */

/* test */"#,
        );

        assert_eq!(decorated.len(), 2);

        let first = &decorated[0];
        assert_eq!(first.text_position(), CommentTextPosition::EndOfLine);
        assert_eq!(first.lines_before(), 0);
        assert_eq!(first.lines_after(), 2);
        assert_eq!(first.preceding_node(), None);
        assert_eq!(
            first.following_node().map(SyntaxNode::kind),
            Some(JsSyntaxKind::JS_MODULE)
        );
        assert_eq!(first.enclosing_node().kind(), JsSyntaxKind::JS_MODULE);

        let second = &decorated[1];
        assert_eq!(second.text_position(), CommentTextPosition::OwnLine);
        assert_eq!(second.lines_before(), 2);
        assert_eq!(second.lines_after(), 0);
        assert_eq!(second.preceding_node(), None);
        assert_eq!(
            first.following_node().map(SyntaxNode::kind),
            Some(JsSyntaxKind::JS_MODULE)
        );
        assert_eq!(second.enclosing_node().kind(), JsSyntaxKind::JS_MODULE);

        assert!(!comments.leading(&root.key()).is_empty());
    }

    fn extract_comments(
        source: &str,
    ) -> (
        JsSyntaxNode,
        Vec<DecoratedComment<JsLanguage>>,
        CommentsMap<SyntaxElementKey, SourceComment<JsLanguage>>,
    ) {
        extract_with_source_map(source, None)
    }

    fn extract_with_source_map(
        source: &str,
        source_map: Option<&TransformSourceMap>,
    ) -> (
        JsSyntaxNode,
        Vec<DecoratedComment<JsLanguage>>,
        CommentsMap<SyntaxElementKey, SourceComment<JsLanguage>>,
    ) {
        let tree = parse_module(source);

        let style = TestCommentStyle::default();
        let builder = CommentsBuilderVisitor::new(&style, source_map);
        let (comments, _) = builder.visit(&tree.syntax());

        (tree.syntax(), style.finish(), comments)
    }

    #[derive(Default)]
    struct TestCommentStyle {
        recorded_comments: RefCell<Vec<DecoratedComment<JsLanguage>>>,
    }

    impl CommentStyle for TestCommentStyle {
        type Language = JsLanguage;

        fn is_suppression(_: &str) -> bool {
            false
        }

        fn get_comment_kind(_: &SyntaxTriviaPieceComments<Self::Language>) -> CommentKind {
            CommentKind::Block
        }

        fn place_comment(
            &self,
            comment: DecoratedComment<Self::Language>,
        ) -> CommentPlacement<Self::Language> {
            self.recorded_comments.borrow_mut().push(comment.clone());

            CommentPlacement::Default(comment)
        }
    }

    impl TestCommentStyle {
        fn finish(self) -> Vec<DecoratedComment<JsLanguage>> {
            self.recorded_comments.take()
        }
    }
}
