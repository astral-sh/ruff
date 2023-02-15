use std::{fmt, ops::Deref};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeOrToken<N, T> {
    Node(N),
    Token(T),
}

impl<N, T> NodeOrToken<N, T> {
    pub fn into_node(self) -> Option<N> {
        match self {
            NodeOrToken::Node(node) => Some(node),
            NodeOrToken::Token(_) => None,
        }
    }

    pub fn into_token(self) -> Option<T> {
        match self {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(token) => Some(token),
        }
    }

    pub fn as_node(&self) -> Option<&N> {
        match self {
            NodeOrToken::Node(node) => Some(node),
            NodeOrToken::Token(_) => None,
        }
    }

    pub fn as_token(&self) -> Option<&T> {
        match self {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(token) => Some(token),
        }
    }
}

impl<N: Deref, T: Deref> NodeOrToken<N, T> {
    pub(crate) fn as_deref(&self) -> NodeOrToken<&N::Target, &T::Target> {
        match self {
            NodeOrToken::Node(node) => NodeOrToken::Node(&**node),
            NodeOrToken::Token(token) => NodeOrToken::Token(&**token),
        }
    }
}

impl<N: fmt::Display, T: fmt::Display> fmt::Display for NodeOrToken<N, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeOrToken::Node(node) => fmt::Display::fmt(node, f),
            NodeOrToken::Token(token) => fmt::Display::fmt(token, f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Next,
    Prev,
}

/// `WalkEvent` describes tree walking process.
#[derive(Debug, Copy, Clone)]
pub enum WalkEvent<T> {
    /// Fired before traversing the node.
    Enter(T),
    /// Fired after the node is traversed.
    Leave(T),
}

impl<T> WalkEvent<T> {
    pub fn map<F: FnOnce(T) -> U, U>(self, f: F) -> WalkEvent<U> {
        match self {
            WalkEvent::Enter(it) => WalkEvent::Enter(f(it)),
            WalkEvent::Leave(it) => WalkEvent::Leave(f(it)),
        }
    }
}

/// There might be zero, one or two leaves at a given offset.
#[derive(Clone, Debug)]
pub enum TokenAtOffset<T> {
    /// No leaves at offset -- possible for the empty file.
    None,
    /// Only a single leaf at offset.
    Single(T),
    /// Offset is exactly between two leaves.
    Between(T, T),
}

impl<T> TokenAtOffset<T> {
    pub fn map<F: Fn(T) -> U, U>(self, f: F) -> TokenAtOffset<U> {
        match self {
            TokenAtOffset::None => TokenAtOffset::None,
            TokenAtOffset::Single(it) => TokenAtOffset::Single(f(it)),
            TokenAtOffset::Between(l, r) => TokenAtOffset::Between(f(l), f(r)),
        }
    }

    /// Convert to option, preferring the right leaf in case of a tie.
    pub fn right_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(node) => Some(node),
            TokenAtOffset::Between(_, right) => Some(right),
        }
    }

    /// Convert to option, preferring the left leaf in case of a tie.
    pub fn left_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(node) => Some(node),
            TokenAtOffset::Between(left, _) => Some(left),
        }
    }
}

impl<T> Iterator for TokenAtOffset<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match std::mem::replace(self, TokenAtOffset::None) {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(node) => {
                *self = TokenAtOffset::None;
                Some(node)
            }
            TokenAtOffset::Between(left, right) => {
                *self = TokenAtOffset::Single(right);
                Some(left)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            TokenAtOffset::None => (0, Some(0)),
            TokenAtOffset::Single(_) => (1, Some(1)),
            TokenAtOffset::Between(_, _) => (2, Some(2)),
        }
    }
}

impl<T> ExactSizeIterator for TokenAtOffset<T> {}

#[cfg(target_pointer_width = "64")]
#[macro_export]
macro_rules! static_assert {
    ($expr:expr) => {
        const _: i32 = 0 / $expr as i32;
    };
}

#[cfg(target_pointer_width = "64")]
pub use static_assert;
