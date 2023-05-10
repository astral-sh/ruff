use crate::builtin::Attributed;
use rustpython_parser_core::source_code::{SourceLocation, SourceLocator, SourceRange};

impl crate::fold::Fold<()> for SourceLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;

    #[cold]
    fn map_user(&mut self, _user: ()) -> Result<Self::TargetU, Self::Error> {
        unreachable!("implemented map_attributed");
    }

    fn map_attributed<T>(
        &mut self,
        node: Attributed<T, ()>,
    ) -> Result<Attributed<T, Self::TargetU>, Self::Error> {
        let start = self.locate(node.range.start());
        let end = self.locate(node.range.end());
        Ok(Attributed {
            range: node.range,
            custom: (start..end).into(),
            node: node.node,
        })
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
