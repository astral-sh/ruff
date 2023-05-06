use crate::attributed::Attributed;
use crate::fold_helpers::Foldable;
use rustpython_compiler_core::{
    text_size::{TextRange, TextSize},
    Location, LocationRange,
};

/// Converts source code byte-offset to Python convention line and column numbers.
#[derive(Default)]
pub struct Locator<'a> {
    source: &'a str,
}

impl<'a> Locator<'a> {
    #[inline]
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn source(&'a self) -> &'a str {
        self.source
    }

    pub fn locate(&mut self, offset: TextSize) -> Location {
        todo!()
    }

    pub fn locate_range(&mut self, range: TextRange) -> LocationRange {
        self.locate(range.start())..self.locate(range.end())
    }

    pub fn locate_ast<X: Foldable<(), LocationRange>>(&mut self, ast: X) -> X::Mapped {
        ast.fold(self).unwrap()
    }
}

impl crate::fold::Fold<()> for Locator<'_> {
    type TargetU = LocationRange;
    type Error = std::convert::Infallible;

    #[cold]
    fn map_user(&mut self, _user: ()) -> Result<Self::TargetU, Self::Error> {
        unreachable!("implemented map_located");
    }

    fn map_located<T>(
        &mut self,
        node: Attributed<T, ()>,
    ) -> Result<Attributed<T, Self::TargetU>, Self::Error> {
        let location = self.locate_range(node.range);
        Ok(Attributed {
            range: node.range,
            custom: location,
            node: node.node,
        })
    }
}
