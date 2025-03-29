use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

impl MarkupKind {
    pub(crate) fn fenced_code_block<T>(self, code: T, language: &str) -> FencedCodeBlock<T>
    where
        T: fmt::Display,
    {
        FencedCodeBlock {
            language,
            code,
            kind: self,
        }
    }
}

pub(crate) struct FencedCodeBlock<'a, T> {
    language: &'a str,
    code: T,
    kind: MarkupKind,
}

impl<T> fmt::Display for FencedCodeBlock<'_, T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            MarkupKind::PlainText => self.code.fmt(f),
            MarkupKind::Markdown => write!(
                f,
                "```{language}\n{code}\n```",
                language = self.language,
                code = self.code
            ),
        }
    }
}
