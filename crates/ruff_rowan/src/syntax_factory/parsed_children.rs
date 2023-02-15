use crate::green::GreenElement;
use crate::syntax_factory::raw_syntax::{RawSyntaxElement, RawSyntaxElementRef};
use crate::SyntaxKind;
use std::iter::FusedIterator;
use std::marker::PhantomData;

/// The parsed children of a node, not accounting for any missing children (required or optional)
#[derive(Debug)]
pub struct ParsedChildren<'a, K> {
    /// Reference to an array containing all children of this node or any of its parents
    all_children: &'a mut Vec<(u64, GreenElement)>,

    /// The index of the first child of this node in the `all_children` array
    first_child: usize,
    ph: PhantomData<K>,
}

impl<'a, K: SyntaxKind> ParsedChildren<'a, K> {
    pub(crate) fn new(all_children: &'a mut Vec<(u64, GreenElement)>, first_child: usize) -> Self {
        Self {
            all_children,
            first_child,
            ph: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        (self.first_child..self.all_children.len()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a, K: SyntaxKind> IntoIterator for ParsedChildren<'a, K> {
    type Item = RawSyntaxElement<K>;
    type IntoIter = ParsedChildrenIntoIterator<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        ParsedChildrenIntoIterator {
            inner: self.all_children.drain(self.first_child..),
            ph: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct ParsedChildrenIntoIterator<'a, K> {
    inner: std::vec::Drain<'a, (u64, GreenElement)>,
    ph: PhantomData<K>,
}

impl<'a, K: SyntaxKind> Iterator for ParsedChildrenIntoIterator<'a, K> {
    type Item = RawSyntaxElement<K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(_, raw)| RawSyntaxElement::from(raw))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'a, K: SyntaxKind> FusedIterator for ParsedChildrenIntoIterator<'a, K> {}

impl<'a, K: SyntaxKind> ExactSizeIterator for ParsedChildrenIntoIterator<'a, K> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a, K: SyntaxKind> DoubleEndedIterator for ParsedChildrenIntoIterator<'a, K> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(_, raw)| RawSyntaxElement::from(raw))
    }
}

impl<'a, K: SyntaxKind> IntoIterator for &'a ParsedChildren<'a, K> {
    type Item = RawSyntaxElementRef<'a, K>;
    type IntoIter = ParsedChildrenIterator<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        ParsedChildrenIterator {
            inner: self.all_children[self.first_child..].iter(),
            ph: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct ParsedChildrenIterator<'a, K> {
    inner: std::slice::Iter<'a, (u64, GreenElement)>,
    ph: PhantomData<K>,
}

impl<'a, K: SyntaxKind> Iterator for ParsedChildrenIterator<'a, K> {
    type Item = RawSyntaxElementRef<'a, K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(_, raw)| RawSyntaxElementRef::from(raw))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'a, K: SyntaxKind> FusedIterator for ParsedChildrenIterator<'a, K> {}

impl<'a, K: SyntaxKind> ExactSizeIterator for ParsedChildrenIterator<'a, K> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a, K: SyntaxKind> DoubleEndedIterator for ParsedChildrenIterator<'a, K> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(_, raw)| RawSyntaxElementRef::from(raw))
    }
}
