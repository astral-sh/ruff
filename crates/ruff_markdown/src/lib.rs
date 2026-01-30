use std::{path::Path, sync::LazyLock};

use regex::Regex;
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
static MARKDOWN_CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    // adapted from blacken-docs
    // https://github.com/adamchainz/blacken-docs/blob/fb107c1dce25f9206e29297aaa1ed7afc2980a5a/src/blacken_docs/__init__.py#L17
    Regex::new(
        r"(?imsx)
                    (?<before>
                        ^(?<indent>\ *)```[^\S\r\n]*
                        (?<lang>(?:python|py|python3|py3|pyi)?)
                        (?:\ .*?)?\n
                    )
                    (?<code>.*?)
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
    let mut formatted = String::with_capacity(source.len());
    let mut last_match = 0;

    for capture in MARKDOWN_CODE_BLOCK.captures_iter(source) {
        let (_, [before, code_indent, language, code, after]) = capture.extract();

        let py_source_type = PySourceType::from_extension(language);
        let unformatted_code = dedent(code);
        let options = settings.to_format_options(py_source_type, &unformatted_code, path);

        // Using `Printed::into_code` requires adding `ruff_formatter` as a direct dependency, and I suspect that Rust can optimize the closure away regardless.
        #[expect(clippy::redundant_closure_for_method_calls)]
        let formatted_code =
            format_module_source(&unformatted_code, options).map(|formatted| formatted.into_code());

        if let Ok(formatted_code) = formatted_code {
            if formatted_code.len() != unformatted_code.len() || formatted_code != *unformatted_code
            {
                let m = capture.get_match();
                formatted.push_str(&source[last_match..m.start()]);

                let indented_code = indent(&formatted_code, code_indent);
                // otherwise I need to deal with a result from write!
                #[expect(clippy::format_push_string)]
                formatted.push_str(&format!("{before}{indented_code}{after}"));

                last_match = m.end();
                changed = true;
            }
        }
    }

    if changed {
        formatted.push_str(&source[last_match..]);
        MarkdownResult::Formatted(formatted)
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

More text.
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @r#"
        This is poorly formatted code:

        ```py
        print("hello")
        ```

        More text.
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

More text.
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_syntax_error() {
        let code = r#"
This is well formatted code:

```py
print "hello"
```

More text.
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_unlabeled_python() {
        let code = r#"
This is poorly formatted code:

```
print( "hello" )
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @r#"
        This is poorly formatted code:

        ```
        print("hello")
        ```
        "#);
    }

    #[test]
    fn format_code_blocks_unlabeled_rust() {
        let code = r#"
This is poorly formatted code:

```
fn (foo: &str) -> &str {
    foo
}
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
            @"Unchanged");
    }
}
