use rustpython_parser_core::{
    source_code::{SourceLocation, SourceLocator, SourceRange},
    text_size::TextRange,
};

impl crate::fold::Fold<TextRange> for SourceLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;
    type UserContext = SourceLocation;

    fn will_map_user(&mut self, user: &TextRange) -> Self::UserContext {
        self.locate(user.start())
    }

    fn map_user(
        &mut self,
        user: TextRange,
        start: Self::UserContext,
    ) -> Result<Self::TargetU, Self::Error> {
        let end = self.locate(user.end());
        Ok((start..end).into())
    }
}
