use std::{path::Path, sync::LazyLock};

use regex::{Captures, Regex};
use ruff_python_ast::PySourceType;
use ruff_python_formatter::format_module_source;
use ruff_python_trivia::textwrap::{dedent, indent};
use ruff_workspace::FormatterSettings;

#[derive(Debug, PartialEq, Eq)]
pub enum MarkdownResult {
    Formatted(String),
    Unchanged,
}

// TODO: account for ~~~ and arbitrary length code fences
// TODO: support code blocks nested inside block quotes, etc
// TODO: support unlabeled code blocks
static MARKDOWN_CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    // adapted from blacken-docs
    // https://github.com/adamchainz/blacken-docs/blob/fb107c1dce25f9206e29297aaa1ed7afc2980a5a/src/blacken_docs/__init__.py#L17
    Regex::new(
        r"(?imx)
                    (?<before>
                        ^(?<indent>\ *)```[^\S\r\n]*
                        (?<lang>python|py|python3|py3|pyi)
                        (?<info>(?:\ .*)?)\n
                    )
                    (?s:(?<code>.*?))
                    (?<after>
                        ^\ *```[^\S\r\n]*$
                    )
                    ",
    )
    .unwrap()
});

pub fn format_code_blocks(
    source: &str,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> MarkdownResult {
    let mut changed = false;
    let formatted_document = MARKDOWN_CODE_BLOCK.replace_all(source, |capture: &Captures| {
        let (
            original,
            [
                before,
                code_indent,
                code_lang,
                info_string,
                unformatted_code,
                after,
            ],
        ) = capture.extract();

        if info_string.to_ascii_lowercase().contains("ruff:skip") {
            return original.to_string();
        }

        let code_block_source_type = if code_lang == "pyi" {
            PySourceType::Stub
        } else {
            PySourceType::Python
        };
        let unformatted_code = dedent(unformatted_code);
        let options = settings.to_format_options(code_block_source_type, &unformatted_code, path);

        // Using `Printed::into_code` requires adding `ruff_formatter` as a direct dependency, and I suspect that Rust can optimize the closure away regardless.
        #[expect(clippy::redundant_closure_for_method_calls)]
        let formatted_code =
            format_module_source(&unformatted_code, options).map(|formatted| formatted.into_code());

        // TODO: figure out how to properly raise errors from inside closure
        if let Ok(formatted_code) = formatted_code {
            if formatted_code.len() == unformatted_code.len() && formatted_code == *unformatted_code
            {
                original.to_string()
            } else {
                changed = true;
                let formatted_code = indent(formatted_code.as_str(), code_indent);
                format!("{before}{formatted_code}{after}")
            }
        } else {
            original.to_string()
        }
    });

    if changed {
        MarkdownResult::Formatted(formatted_document.to_string())
    } else {
        MarkdownResult::Unchanged
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_workspace::FormatterSettings;

    use crate::{MarkdownResult, format_code_blocks};

    impl std::fmt::Display for MarkdownResult {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Formatted(source) => write!(f, "{source}"),
                Self::Unchanged => write!(f, "Unchanged"),
            }
        }
    }

    #[test]
    fn format_code_blocks_basic() {
        let code = r#"
This is poorly formatted code:

```py
print( "hello" )
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @r#"
        This is poorly formatted code:

        ```py
        print("hello")
        ```
        "#
        );
    }

    #[test]
    fn format_code_blocks_unchanged() {
        let code = r#"
This is well formatted code:

```py
print("hello")
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_skipped() {
        let code = r#"
This is intentionally poorly formatted code:

```py ruff:skip
print(
      "hello"   )
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @"Unchanged");
    }
}
