use crate::attributed::Attributed;
use rustpython_parser_core::source_code::{SourceLocator, SourceRange};

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
