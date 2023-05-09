use rustpython_parser_core::{
    source_code::{SourceLocation, SourceRange},
    text_size::{TextRange, TextSize},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Attributed<T, U = ()> {
    pub range: TextRange,
    pub custom: U,
    pub node: T,
}

impl<T, U> Attributed<T, U> {
    /// Returns the node
    #[inline]
    pub fn node(&self) -> &T {
        &self.node
    }

    /// Returns the `range` of the node. The range offsets are absolute to the start of the document.
    #[inline]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns the absolute start position of the node from the beginning of the document.
    #[inline]
    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    /// Returns the absolute position at which the node ends in the source document.
    #[inline]
    pub const fn end(&self) -> TextSize {
        self.range.end()
    }
}

impl<T> Attributed<T, ()> {
    /// Creates a new node that spans the position specified by `range`.
    pub fn new(range: impl Into<TextRange>, node: T) -> Self {
        Self {
            range: range.into(),
            custom: (),
            node,
        }
    }

    /// Consumes self and returns the node.
    #[inline]
    pub fn into_node(self) -> T {
        self.node
    }
}

impl<T> Attributed<T, SourceRange> {
    /// Returns the absolute start position of the node from the beginning of the document.
    #[inline]
    pub const fn location(&self) -> SourceLocation {
        self.custom.start
    }

    /// Returns the absolute position at which the node ends in the source document.
    #[inline]
    pub const fn end_location(&self) -> Option<SourceLocation> {
        self.custom.end
    }
}

impl<T, U> std::ops::Deref for Attributed<T, U> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}
