use crate::prelude::*;
use crate::{write, CstFormatContext, GroupId};
use ruff_rowan::{AstNode, AstSeparatedElement, SyntaxResult, SyntaxToken};

pub trait FormatSeparatedElementRule<N>
where
    N: AstNode,
{
    type Context;
    type FormatNode<'a>: Format<Self::Context>
    where
        N: 'a;
    type FormatSeparator<'a>: Format<Self::Context>
    where
        N: 'a;

    fn format_node<'a>(&self, node: &'a N) -> Self::FormatNode<'a>;
    fn format_separator<'a>(
        &self,
        separator: &'a SyntaxToken<N::Language>,
    ) -> Self::FormatSeparator<'a>;
}

/// Formats a single element inside a separated list.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FormatSeparatedElement<N, R>
where
    N: AstNode,
    R: FormatSeparatedElementRule<N>,
{
    element: AstSeparatedElement<N::Language, N>,
    rule: R,
    is_last: bool,
    /// The separator to write if the element has no separator yet.
    separator: &'static str,
    options: FormatSeparatedOptions,
}

impl<N, R> FormatSeparatedElement<N, R>
where
    N: AstNode,
    R: FormatSeparatedElementRule<N>,
{
    /// Returns the node belonging to the element.
    pub fn node(&self) -> SyntaxResult<&N> {
        self.element.node()
    }
}

impl<N, R, C> Format<C> for FormatSeparatedElement<N, R>
where
    N: AstNode,
    N::Language: 'static,
    R: FormatSeparatedElementRule<N, Context = C>,
    C: CstFormatContext<Language = N::Language>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        let node = self.element.node()?;
        let separator = self.element.trailing_separator()?;

        let format_node = self.rule.format_node(node);

        if !self.options.nodes_grouped {
            format_node.fmt(f)?;
        } else {
            group(&format_node).fmt(f)?;
        }

        // Reuse the existing trailing separator or create it if it wasn't in the
        // input source. Only print the last trailing token if the outer group breaks
        if let Some(separator) = separator {
            let format_separator = self.rule.format_separator(separator);

            if self.is_last {
                match self.options.trailing_separator {
                    TrailingSeparator::Allowed => {
                        // Use format_replaced instead of wrapping the result of format_token
                        // in order to remove only the token itself when the group doesn't break
                        // but still print its associated trivia unconditionally
                        format_only_if_breaks(separator, &format_separator)
                            .with_group_id(self.options.group_id)
                            .fmt(f)?;
                    }
                    TrailingSeparator::Mandatory => {
                        write!(f, [format_separator])?;
                    }
                    TrailingSeparator::Disallowed => {
                        // A trailing separator was present where it wasn't allowed, opt out of formatting
                        return Err(FormatError::SyntaxError);
                    }
                    TrailingSeparator::Omit => {
                        write!(f, [format_removed(separator)])?;
                    }
                }
            } else {
                write!(f, [format_separator])?;
            }
        } else if self.is_last {
            match self.options.trailing_separator {
                TrailingSeparator::Allowed => {
                    write!(
                        f,
                        [if_group_breaks(&text(self.separator))
                            .with_group_id(self.options.group_id)]
                    )?;
                }
                TrailingSeparator::Mandatory => {
                    text(self.separator).fmt(f)?;
                }
                TrailingSeparator::Omit | TrailingSeparator::Disallowed => { /* no op */ }
            }
        } else {
            unreachable!(
                "This is a syntax error, separator must be present between every two elements"
            );
        };

        Ok(())
    }
}

/// Iterator for formatting separated elements. Prints the separator between each element and
/// inserts a trailing separator if necessary
pub struct FormatSeparatedIter<I, Node, Rule>
where
    Node: AstNode,
{
    next: Option<AstSeparatedElement<Node::Language, Node>>,
    rule: Rule,
    inner: I,
    separator: &'static str,
    options: FormatSeparatedOptions,
}

impl<I, Node, Rule> FormatSeparatedIter<I, Node, Rule>
where
    Node: AstNode,
{
    pub fn new(inner: I, separator: &'static str, rule: Rule) -> Self {
        Self {
            inner,
            rule,
            separator,
            next: None,
            options: FormatSeparatedOptions::default(),
        }
    }

    /// Wraps every node inside of a group
    pub fn nodes_grouped(mut self) -> Self {
        self.options.nodes_grouped = true;
        self
    }

    pub fn with_trailing_separator(mut self, separator: TrailingSeparator) -> Self {
        self.options.trailing_separator = separator;
        self
    }

    #[allow(unused)]
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.options.group_id = group_id;
        self
    }
}

impl<I, Node, Rule> Iterator for FormatSeparatedIter<I, Node, Rule>
where
    Node: AstNode,
    I: Iterator<Item = AstSeparatedElement<Node::Language, Node>>,
    Rule: FormatSeparatedElementRule<Node> + Clone,
{
    type Item = FormatSeparatedElement<Node, Rule>;

    fn next(&mut self) -> Option<Self::Item> {
        let element = self.next.take().or_else(|| self.inner.next())?;

        self.next = self.inner.next();
        let is_last = self.next.is_none();

        Some(FormatSeparatedElement {
            element,
            rule: self.rule.clone(),
            is_last,
            separator: self.separator,
            options: self.options,
        })
    }
}

impl<I, Node, Rule> std::iter::FusedIterator for FormatSeparatedIter<I, Node, Rule>
where
    Node: AstNode,
    I: Iterator<Item = AstSeparatedElement<Node::Language, Node>> + std::iter::FusedIterator,
    Rule: FormatSeparatedElementRule<Node> + Clone,
{
}

impl<I, Node, Rule> std::iter::ExactSizeIterator for FormatSeparatedIter<I, Node, Rule>
where
    Node: AstNode,
    I: Iterator<Item = AstSeparatedElement<Node::Language, Node>> + ExactSizeIterator,
    Rule: FormatSeparatedElementRule<Node> + Clone,
{
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum TrailingSeparator {
    /// A trailing separator is allowed and preferred
    #[default]
    Allowed,

    /// A trailing separator is not allowed
    Disallowed,

    /// A trailing separator is mandatory for the syntax to be correct
    Mandatory,

    /// A trailing separator might be present, but the consumer
    /// decides to remove it
    Omit,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct FormatSeparatedOptions {
    trailing_separator: TrailingSeparator,
    group_id: Option<GroupId>,
    nodes_grouped: bool,
}
