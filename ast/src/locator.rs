use crate::attributed::Attributed;
use crate::fold_helpers::Foldable;
use crate::location::SourceRange;
use rustpython_compiler_core::SourceLocator;

pub fn locate<X: Foldable<(), SourceRange>>(locator: &mut SourceLocator, ast: X) -> X::Mapped {
    ast.fold(locator).unwrap()
}

impl crate::fold::Fold<()> for SourceLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;

    #[cold]
    fn map_user(&mut self, _user: ()) -> Result<Self::TargetU, Self::Error> {
        unreachable!("implemented map_located");
    }

    fn map_located<T>(
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
