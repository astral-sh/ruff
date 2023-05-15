use rustpython_parser_core::{
    source_code::{SourceLocator, SourceRange},
    text_size::TextRange,
};

impl crate::fold::Fold<TextRange> for SourceLocator<'_> {
    type TargetU = SourceRange;
    type Error = std::convert::Infallible;

    fn map_user(&mut self, user: TextRange) -> Result<Self::TargetU, Self::Error> {
        let start = self.locate(user.start());
        let end = self.locate(user.end());
        Ok((start..end).into())
    }
}
