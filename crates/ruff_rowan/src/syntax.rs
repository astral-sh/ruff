mod element;
mod node;
mod rewriter;
mod token;
mod trivia;

use crate::{AstNode, RawSyntaxKind};
pub use element::{SyntaxElement, SyntaxElementKey};
pub(crate) use node::SyntaxSlots;
pub use node::{
    Preorder, PreorderWithTokens, SendNode, SyntaxElementChildren, SyntaxNode, SyntaxNodeChildren,
    SyntaxNodeOptionExt, SyntaxSlot,
};
pub use rewriter::{SyntaxRewriter, VisitNodeSignal};
use std::fmt;
use std::fmt::Debug;
pub use token::SyntaxToken;
pub use trivia::{
    chain_trivia_pieces, ChainTriviaPiecesIterator, SyntaxTrivia, SyntaxTriviaPiece,
    SyntaxTriviaPieceComments, SyntaxTriviaPieceNewline, SyntaxTriviaPieceSkipped,
    SyntaxTriviaPieceWhitespace, SyntaxTriviaPiecesIterator, TriviaPiece, TriviaPieceKind,
};

/// Type tag for each node or token of a language
pub trait SyntaxKind: fmt::Debug + PartialEq + Copy {
    const TOMBSTONE: Self;
    const EOF: Self;

    /// Returns `true` if this is a kind of a bogus node.
    fn is_bogus(&self) -> bool;

    /// Converts this into to the best matching bogus node kind.
    fn to_bogus(&self) -> Self;

    /// Converts this kind to a raw syntax kind.
    fn to_raw(&self) -> RawSyntaxKind;

    /// Creates a syntax kind from a raw kind.
    fn from_raw(raw: RawSyntaxKind) -> Self;

    /// Returns `true` if this kind is for a root node.
    fn is_root(&self) -> bool;

    /// Returns `true` if this kind is a list node.
    fn is_list(&self) -> bool;

    /// Returns a string for keywords and punctuation tokens or `None` otherwise.
    fn to_string(&self) -> Option<&'static str>;
}

pub trait Language: Sized + Clone + Copy + fmt::Debug + Eq + Ord + std::hash::Hash {
    type Kind: SyntaxKind;
    type Root: AstNode<Language = Self> + Clone + Eq + fmt::Debug;
}

/// A list of `SyntaxNode`s and/or `SyntaxToken`s
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SyntaxList<L: Language> {
    list: SyntaxNode<L>,
}

impl<L: Language> SyntaxList<L> {
    /// Creates a new list wrapping a List `SyntaxNode`
    fn new(node: SyntaxNode<L>) -> Self {
        Self { list: node }
    }

    /// Iterates over the elements in the list.
    pub fn iter(&self) -> SyntaxSlots<L> {
        self.list.slots()
    }

    /// Returns the number of items in this list
    pub fn len(&self) -> usize {
        self.list.slots().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn first(&self) -> Option<SyntaxSlot<L>> {
        self.list.slots().next()
    }

    pub fn last(&self) -> Option<SyntaxSlot<L>> {
        self.list.slots().last()
    }

    pub fn node(&self) -> &SyntaxNode<L> {
        &self.list
    }

    pub fn into_node(self) -> SyntaxNode<L> {
        self.list
    }
}

impl<L: Language> IntoIterator for &SyntaxList<L> {
    type Item = SyntaxSlot<L>;
    type IntoIter = SyntaxSlots<L>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<L: Language> IntoIterator for SyntaxList<L> {
    type Item = SyntaxSlot<L>;
    type IntoIter = SyntaxSlots<L>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextRange;

    use crate::raw_language::{RawLanguageKind, RawSyntaxTreeBuilder};
    use crate::syntax::TriviaPiece;
    use crate::Direction;

    #[test]
    fn empty_list() {
        let mut builder: RawSyntaxTreeBuilder = RawSyntaxTreeBuilder::new();
        builder.start_node(RawLanguageKind::EXPRESSION_LIST);
        builder.finish_node();
        let list = builder.finish().into_list();

        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        assert_eq!(list.first(), None);
        assert_eq!(list.last(), None);

        assert_eq!(list.iter().collect::<Vec<_>>(), Vec::default());
    }

    #[test]
    fn node_list() {
        let mut builder = RawSyntaxTreeBuilder::new();

        builder.start_node(RawLanguageKind::EXPRESSION_LIST);

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "1");
        builder.finish_node();

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "2");
        builder.finish_node();

        builder.finish_node();

        let node = builder.finish();
        let list = node.into_list();

        assert!(!list.is_empty());
        assert_eq!(list.len(), 2);

        let first = list.first().and_then(|e| e.into_node()).unwrap();
        assert_eq!(first.kind(), RawLanguageKind::LITERAL_EXPRESSION);
        assert_eq!(first.text(), "1");

        let last = list.last().and_then(|e| e.into_node()).unwrap();
        assert_eq!(last.kind(), RawLanguageKind::LITERAL_EXPRESSION);
        assert_eq!(last.text(), "2");

        let node_texts: Vec<_> = list
            .iter()
            .map(|e| e.into_node().map(|n| n.text().to_string()))
            .collect();

        assert_eq!(
            node_texts,
            vec![Some(String::from("1")), Some(String::from("2"))]
        )
    }

    #[test]
    fn node_or_token_list() {
        let mut builder = RawSyntaxTreeBuilder::new();

        builder.start_node(RawLanguageKind::SEPARATED_EXPRESSION_LIST);

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "1");
        builder.finish_node();

        builder.token(RawLanguageKind::NUMBER_TOKEN, ",");

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "2");
        builder.finish_node();

        builder.finish_node();

        let node = builder.finish();
        let list = node.into_list();

        assert!(!list.is_empty());
        assert_eq!(list.len(), 3);

        let first = list.first().and_then(|e| e.into_node()).unwrap();
        assert_eq!(first.kind(), RawLanguageKind::LITERAL_EXPRESSION);
        assert_eq!(first.text(), "1");

        let last = list.last().and_then(|e| e.into_node()).unwrap();
        assert_eq!(last.kind(), RawLanguageKind::LITERAL_EXPRESSION);
        assert_eq!(last.text(), "2");

        let kinds: Vec<_> = list.iter().map(|e| e.kind()).collect();

        assert_eq!(
            kinds,
            vec![
                Some(RawLanguageKind::LITERAL_EXPRESSION),
                Some(RawLanguageKind::NUMBER_TOKEN),
                Some(RawLanguageKind::LITERAL_EXPRESSION)
            ]
        )
    }

    #[test]
    fn siblings() {
        let mut builder = RawSyntaxTreeBuilder::new();

        // list
        builder.start_node(RawLanguageKind::SEPARATED_EXPRESSION_LIST);

        // element 1
        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "a");
        builder.finish_node();

        // element 2
        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "b");
        builder.finish_node();

        // Missing ,

        // element 3
        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::NUMBER_TOKEN, "c");
        builder.finish_node();

        builder.finish_node();

        let root = builder.finish();

        let first = root.children().next().unwrap();
        assert_eq!(first.text().to_string(), "a");
        assert_eq!(
            first.next_sibling().map(|e| e.text().to_string()),
            Some(String::from("b"))
        );

        let second = root.children().nth(1).unwrap();
        assert_eq!(second.text().to_string(), "b");

        // Skips the missing element
        assert_eq!(
            second.next_sibling().map(|e| e.text().to_string()),
            Some(String::from("c"))
        );

        assert_eq!(
            second.prev_sibling().map(|e| e.text().to_string()),
            Some(String::from("a"))
        );

        let last = root.children().last().unwrap();
        assert_eq!(last.text(), "c");
        assert_eq!(last.next_sibling(), None);
        assert_eq!(
            last.prev_sibling().map(|e| e.text().to_string()),
            Some(String::from("b"))
        );

        assert_eq!(
            first
                .siblings(Direction::Next)
                .map(|s| s.text().to_string())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );

        assert_eq!(
            last.siblings(Direction::Prev)
                .map(|s| s.text().to_string())
                .collect::<Vec<_>>(),
            vec!["c", "b", "a"]
        );
    }

    #[test]
    fn siblings_with_tokens() {
        let mut builder = RawSyntaxTreeBuilder::new();

        builder.start_node(RawLanguageKind::ROOT);

        builder.token(RawLanguageKind::FOR_KW, "for");
        builder.token(RawLanguageKind::L_PAREN_TOKEN, "(");
        builder.token(RawLanguageKind::SEMICOLON_TOKEN, ";");

        builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        builder.token(RawLanguageKind::STRING_TOKEN, "x");
        builder.finish_node();

        builder.token(RawLanguageKind::SEMICOLON_TOKEN, ";");
        builder.token(RawLanguageKind::R_PAREN_TOKEN, ")");

        builder.finish_node();

        let root = builder.finish();

        let first_semicolon = root
            .children_with_tokens()
            .nth(2)
            .and_then(|e| e.into_token())
            .unwrap();

        assert_eq!(first_semicolon.text(), ";");

        assert_eq!(
            first_semicolon
                .siblings_with_tokens(Direction::Next)
                .map(|e| e.to_string())
                .collect::<Vec<_>>(),
            vec!["x", ";", ")"]
        );

        assert_eq!(
            first_semicolon.next_sibling_or_token(),
            first_semicolon.siblings_with_tokens(Direction::Next).next()
        );
        assert_eq!(
            first_semicolon.prev_sibling_or_token(),
            first_semicolon.siblings_with_tokens(Direction::Prev).next()
        );
    }

    #[test]
    pub fn syntax_text_and_len() {
        let mut builder = RawSyntaxTreeBuilder::new();
        builder.start_node(RawLanguageKind::ROOT);
        builder.token_with_trivia(
            RawLanguageKind::LET_TOKEN,
            "\n\t let \t\t",
            &[TriviaPiece::whitespace(3)],
            &[TriviaPiece::whitespace(3)],
        );
        builder.finish_node();

        // // Node texts

        let node = builder.finish();
        assert_eq!("\n\t let \t\t", node.text());
        assert_eq!("let", node.text_trimmed());
        assert_eq!("\n\t ", node.first_leading_trivia().unwrap().text());
        assert_eq!(" \t\t", node.last_trailing_trivia().unwrap().text());

        // Token texts

        let token = node.first_token().unwrap();
        assert_eq!("\n\t let \t\t", token.text());
        assert_eq!("let", token.text_trimmed());
        assert_eq!("\n\t ", token.leading_trivia().text());
        assert_eq!(" \t\t", token.trailing_trivia().text());
    }

    #[test]
    pub fn syntax_range() {
        let mut builder = RawSyntaxTreeBuilder::new();
        builder.start_node(RawLanguageKind::ROOT);
        builder.token_with_trivia(
            RawLanguageKind::LET_TOKEN,
            "\n\t let \t\t",
            &[TriviaPiece::whitespace(3)],
            &[TriviaPiece::whitespace(3)],
        );
        builder.token_with_trivia(
            RawLanguageKind::LET_TOKEN,
            "a ",
            &[TriviaPiece::whitespace(0)],
            &[TriviaPiece::whitespace(1)],
        );
        builder.token_with_trivia(
            RawLanguageKind::EQUAL_TOKEN,
            "\n=\n",
            &[TriviaPiece::whitespace(1)],
            &[TriviaPiece::whitespace(1)],
        );
        builder.token(RawLanguageKind::NUMBER_TOKEN, "1");
        builder.token_with_trivia(
            RawLanguageKind::SEMICOLON_TOKEN,
            ";\t\t",
            &[],
            &[TriviaPiece::whitespace(2)],
        );
        builder.finish_node();

        let node = builder.finish();

        // Node Ranges

        assert_eq!(TextRange::new(0.into(), 18.into()), node.text_range());
        assert_eq!(
            TextRange::new(3.into(), 16.into()),
            node.text_trimmed_range()
        );
        assert_eq!(
            TextRange::new(0.into(), 3.into()),
            node.first_leading_trivia().unwrap().text_range()
        );
        assert_eq!(
            TextRange::new(16.into(), 18.into()),
            node.last_trailing_trivia().unwrap().text_range()
        );

        // as NodeOrToken

        let eq_token = node
            .descendants_with_tokens(Direction::Next)
            .find(|x| x.kind() == RawLanguageKind::EQUAL_TOKEN)
            .unwrap();

        assert_eq!(TextRange::new(11.into(), 14.into()), eq_token.text_range());
        assert_eq!(
            TextRange::new(12.into(), 13.into()),
            eq_token.text_trimmed_range()
        );
        assert_eq!(
            TextRange::new(11.into(), 12.into()),
            eq_token.leading_trivia().unwrap().text_range()
        );
        assert_eq!(
            TextRange::new(13.into(), 14.into()),
            eq_token.trailing_trivia().unwrap().text_range()
        );

        // as Token

        let eq_token = eq_token.as_token().unwrap();
        assert_eq!(TextRange::new(11.into(), 14.into()), eq_token.text_range());
        assert_eq!(
            TextRange::new(12.into(), 13.into()),
            eq_token.text_trimmed_range()
        );
        assert_eq!(
            TextRange::new(11.into(), 12.into()),
            eq_token.leading_trivia().text_range()
        );
        assert_eq!(
            TextRange::new(13.into(), 14.into()),
            eq_token.trailing_trivia().text_range()
        );
    }

    #[test]
    pub fn syntax_trivia_pieces() {
        use crate::*;
        let node = RawSyntaxTreeBuilder::wrap_with_node(RawLanguageKind::ROOT, |builder| {
            builder.token_with_trivia(
                RawLanguageKind::LET_TOKEN,
                "\n\t /**/let \t\t",
                &[
                    TriviaPiece::whitespace(3),
                    TriviaPiece::single_line_comment(4),
                ],
                &[TriviaPiece::whitespace(3)],
            );
        });

        let pieces: Vec<_> = node.first_leading_trivia().unwrap().pieces().collect();
        assert_eq!(2, pieces.len());

        assert_eq!("\n\t ", pieces[0].text());
        assert_eq!(TextSize::from(3), pieces[0].text_len());
        assert_eq!(TextRange::new(0.into(), 3.into()), pieces[0].text_range());
        assert!(pieces[0].is_whitespace());

        assert_eq!("/**/", pieces[1].text());
        assert_eq!(TextSize::from(4), pieces[1].text_len());
        assert_eq!(TextRange::new(3.into(), 7.into()), pieces[1].text_range());
        assert!(pieces[1].is_comments());

        let pieces_rev: Vec<_> = node
            .first_leading_trivia()
            .unwrap()
            .pieces()
            .rev()
            .collect();

        assert_eq!(2, pieces_rev.len());
        assert_eq!("/**/", pieces_rev[0].text());
        assert_eq!("\n\t ", pieces_rev[1].text());
    }
}
