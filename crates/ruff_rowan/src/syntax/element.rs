use crate::syntax::SyntaxTrivia;
use crate::{cursor, Language, NodeOrToken, SyntaxNode, SyntaxToken};
use ruff_text_size::{TextRange, TextSize};
use std::iter;
use std::ptr::NonNull;

pub type SyntaxElement<L> = NodeOrToken<SyntaxNode<L>, SyntaxToken<L>>;

impl<L: Language> SyntaxElement<L> {
    pub fn key(&self) -> SyntaxElementKey {
        match self {
            NodeOrToken::Node(it) => it.key(),
            NodeOrToken::Token(it) => it.key(),
        }
    }

    pub fn text_range(&self) -> TextRange {
        match self {
            NodeOrToken::Node(it) => it.text_range(),
            NodeOrToken::Token(it) => it.text_range(),
        }
    }

    pub fn text_trimmed_range(&self) -> TextRange {
        match self {
            NodeOrToken::Node(it) => it.text_trimmed_range(),
            NodeOrToken::Token(it) => it.text_trimmed_range(),
        }
    }

    pub fn leading_trivia(&self) -> Option<SyntaxTrivia<L>> {
        match self {
            NodeOrToken::Node(it) => it.first_leading_trivia(),
            NodeOrToken::Token(it) => Some(it.leading_trivia()),
        }
    }

    pub fn trailing_trivia(&self) -> Option<SyntaxTrivia<L>> {
        match self {
            NodeOrToken::Node(it) => it.last_trailing_trivia(),
            NodeOrToken::Token(it) => Some(it.trailing_trivia()),
        }
    }

    pub fn kind(&self) -> L::Kind {
        match self {
            NodeOrToken::Node(it) => it.kind(),
            NodeOrToken::Token(it) => it.kind(),
        }
    }

    pub fn parent(&self) -> Option<SyntaxNode<L>> {
        match self {
            NodeOrToken::Node(it) => it.parent(),
            NodeOrToken::Token(it) => it.parent(),
        }
    }

    pub(crate) fn index(&self) -> usize {
        match self {
            NodeOrToken::Node(it) => it.index(),
            NodeOrToken::Token(it) => it.index(),
        }
    }

    pub fn ancestors(&self) -> impl Iterator<Item = SyntaxNode<L>> {
        let first = match self {
            NodeOrToken::Node(it) => Some(it.clone()),
            NodeOrToken::Token(it) => it.parent(),
        };
        iter::successors(first, SyntaxNode::parent)
    }

    pub fn next_sibling_or_token(&self) -> Option<SyntaxElement<L>> {
        match self {
            NodeOrToken::Node(it) => it.next_sibling_or_token(),
            NodeOrToken::Token(it) => it.next_sibling_or_token(),
        }
    }

    pub fn prev_sibling_or_token(&self) -> Option<SyntaxElement<L>> {
        match self {
            NodeOrToken::Node(it) => it.prev_sibling_or_token(),
            NodeOrToken::Token(it) => it.prev_sibling_or_token(),
        }
    }

    #[must_use = "syntax elements are immutable, the result of update methods must be propagated to have any effect"]
    pub fn detach(self) -> Self {
        match self {
            NodeOrToken::Node(it) => Self::Node(it.detach()),
            NodeOrToken::Token(it) => Self::Token(it.detach()),
        }
    }
}

impl<L: Language> From<cursor::SyntaxElement> for SyntaxElement<L> {
    fn from(raw: cursor::SyntaxElement) -> SyntaxElement<L> {
        match raw {
            NodeOrToken::Node(it) => NodeOrToken::Node(it.into()),
            NodeOrToken::Token(it) => NodeOrToken::Token(it.into()),
        }
    }
}

impl<L: Language> From<SyntaxElement<L>> for cursor::SyntaxElement {
    fn from(element: SyntaxElement<L>) -> cursor::SyntaxElement {
        match element {
            NodeOrToken::Node(it) => NodeOrToken::Node(it.into()),
            NodeOrToken::Token(it) => NodeOrToken::Token(it.into()),
        }
    }
}

impl<L: Language> From<SyntaxToken<L>> for SyntaxElement<L> {
    fn from(token: SyntaxToken<L>) -> SyntaxElement<L> {
        NodeOrToken::Token(token)
    }
}

impl<L: Language> From<SyntaxNode<L>> for SyntaxElement<L> {
    fn from(node: SyntaxNode<L>) -> SyntaxElement<L> {
        NodeOrToken::Node(node)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SyntaxElementKey {
    node_data: NonNull<()>,
    offset: TextSize,
}

impl SyntaxElementKey {
    pub(crate) fn new(node_data: NonNull<()>, offset: TextSize) -> Self {
        Self { node_data, offset }
    }
}
