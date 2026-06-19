use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

/// Capabilities available to the Markdown renderer.
#[derive(Copy, Clone, Debug, Default)]
pub struct MarkdownRenderOptions {
    html_ul: bool,
}

impl MarkdownRenderOptions {
    /// Creates options that do not enable any renderer-generated HTML.
    pub const fn new() -> Self {
        Self { html_ul: false }
    }

    /// Enables renderer-generated HTML `<ul>` elements.
    #[must_use]
    pub const fn with_html_ul(mut self, supported: bool) -> Self {
        self.html_ul = supported;
        self
    }

    pub(crate) const fn supports_html_ul(self) -> bool {
        self.html_ul
    }
}

impl MarkupKind {
    pub(crate) const fn fenced_code_block<T>(
        self,
        code: T,
        language: &str,
    ) -> FencedCodeBlock<'_, T>
    where
        T: fmt::Display,
    {
        FencedCodeBlock {
            language,
            code,
            kind: self,
        }
    }

    pub(crate) const fn horizontal_line(self) -> HorizontalLine {
        HorizontalLine { kind: self }
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

#[derive(Debug, Copy, Clone)]
pub(crate) struct HorizontalLine {
    kind: MarkupKind,
}

impl fmt::Display for HorizontalLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.kind {
            MarkupKind::PlainText => {
                f.write_str("\n---------------------------------------------\n")
            }
            MarkupKind::Markdown => {
                write!(f, "\n---\n")
            }
        }
    }
}
