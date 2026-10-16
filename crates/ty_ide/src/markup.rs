use std::fmt;
use std::fmt::Formatter;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MarkupKind {
    PlainText,
    Markdown,
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
            MarkupKind::Markdown => {
                let code = self.code.to_string();
                // Type names can contain arbitrary text, including Markdown fences.
                // Inspect the complete rendered value so a fence cannot span display writes.
                let longest_backticks = code
                    .split(|character| character != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or(0);
                let fence = "`".repeat(longest_backticks.max(2) + 1);
                write!(f, "{fence}{}\n{code}\n{fence}", self.language)
            }
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

#[cfg(test)]
mod tests {
    use super::MarkupKind;

    #[test]
    fn ordinary_code_fence() {
        assert_eq!(
            MarkupKind::Markdown
                .fenced_code_block("int", "python")
                .to_string(),
            "```python\nint\n```"
        );
    }

    #[test]
    fn embedded_code_fences() {
        for length in [3, 4, 7] {
            let embedded = "`".repeat(length);
            let code = format!("name\n{embedded}\n![image](https://example.invalid/)\n{embedded}");
            let fence = "`".repeat(length + 1);
            assert_eq!(
                MarkupKind::Markdown
                    .fenced_code_block(&code, "python")
                    .to_string(),
                format!("{fence}python\n{code}\n{fence}")
            );
            assert_eq!(
                MarkupKind::PlainText
                    .fenced_code_block(&code, "python")
                    .to_string(),
                code
            );
        }
    }

    #[test]
    fn fence_spanning_formatted_values() {
        let code = std::fmt::from_fn(|f| {
            f.write_str("``")?;
            f.write_str("```")
        });

        assert_eq!(
            MarkupKind::Markdown
                .fenced_code_block(code, "python")
                .to_string(),
            "``````python\n`````\n``````"
        );
    }
}
