use std::{path::Path, sync::LazyLock};

use regex::Regex;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::{FormatModuleError, format_module_source};
use ruff_python_trivia::textwrap::{dedent, indent};
use ruff_source_file::{Line, UniversalNewlines};
use ruff_text_size::{TextLen, TextRange, TextSize};
use ruff_workspace::FormatterSettings;

#[derive(Debug, PartialEq, Eq)]
pub enum MarkdownResult {
    Formatted(String),
    Unchanged,
}

// TODO: support code blocks nested inside block quotes, etc
static MARKDOWN_CODE_FENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
            ^
            (?<indent>\s*)
            (?<fence>(?:```+|~~~+))\s*
            \{?(?<language>(?:\w+)?)\}?\s*
            (?<info>(?:.*))\s*
            $
        ",
    )
    .unwrap()
});

static OFF_ON_DIRECTIVES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?imx)
            ^
            \s*<!--\s*(?:blacken-docs|fmt)\s*:\s*(?<action>off|on)\s*-->
        ",
    )
    .unwrap()
});

#[derive(Debug, Default, PartialEq, Eq)]
enum MarkdownState {
    #[default]
    On,
    Off,
}

/// Returns `true` if code blocks that fail to format, for example because they contain a syntax
/// error, are reported as errors instead of being skipped.
///
/// See <https://github.com/astral-sh/ruff/issues/25523>.
const fn is_code_block_errors_enabled(settings: &FormatterSettings) -> bool {
    settings.preview.is_enabled()
}

fn is_closing_code_fence(line: &str, opening_fence: &str) -> bool {
    let Some(fence_byte) = opening_fence.as_bytes().first().copied() else {
        return false;
    };

    let line = line.trim_start();
    let fence_len = line
        .as_bytes()
        .iter()
        .take_while(|&&byte| byte == fence_byte)
        .count();

    fence_len >= opening_fence.len() && line[fence_len..].chars().all(|ch| matches!(ch, ' ' | '\t'))
}

pub fn format_code_blocks(
    source: &str,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> Result<MarkdownResult, FormatModuleError> {
    let mut state = MarkdownState::On;
    let mut changed = false;
    let mut formatted = String::with_capacity(source.len());
    let mut last_match = TextSize::ZERO;

    let mut lines = source.universal_newlines().peekable();
    while let Some(line) = lines.next() {
        // Toggle code block formatting off/on
        if let Some(capture) = OFF_ON_DIRECTIVES.captures(&line) {
            let (_, [action]) = capture.extract();
            state = match action {
                "off" => MarkdownState::Off,
                "on" => MarkdownState::On,
                _ => state,
            };
        // Process code blocks
        } else if let Some(opening_capture) = MARKDOWN_CODE_FENCE.captures(&line) {
            let (_, [code_indent, opening_fence, language, _info]) = opening_capture.extract();
            let start = lines.peek().map_or_default(Line::start);

            // Consume lines until reaching the matching/ending code fence
            for code_line in lines.by_ref() {
                if !is_closing_code_fence(&code_line, opening_fence) {
                    continue;
                }

                // Found the matching end of the code block
                if state != MarkdownState::On {
                    break;
                }

                // Maybe python, try formatting it
                let language = language.to_ascii_lowercase();
                let SourceType::Python(py_source_type) =
                    settings.extension.get_source_type_by_extension(&language)
                else {
                    break;
                };

                let end = code_line.start();
                let block = &source[TextRange::new(start, end)];
                let unformatted_code = dedent(block);

                let formatted_code = match language.as_str() {
                    "python" | "py" | "python3" | "py3" | "pyi" => {
                        let options =
                            settings.to_format_options(py_source_type, &unformatted_code, path);
                        format_module_source(&unformatted_code, options)
                            .map(|formatted| Some(formatted.into_code()))
                    }
                    "pycon" => format_pycon_block(&unformatted_code, path, settings),
                    _ => Ok(None),
                };

                let formatted_code = match formatted_code {
                    Ok(formatted_code) => formatted_code,
                    Err(mut error) if is_code_block_errors_enabled(settings) => {
                        relocate_parse_error(&mut error, block, &unformatted_code, start);
                        return Err(error);
                    }
                    Err(_) => None,
                };

                // Formatting produced changes
                if let Some(formatted_code) = formatted_code
                    && (formatted_code.len() != unformatted_code.len()
                        || formatted_code != *unformatted_code)
                {
                    formatted.push_str(&source[TextRange::new(last_match, start)]);
                    let formatted_code = indent(&formatted_code, code_indent);
                    formatted.push_str(&formatted_code);
                    last_match = end;
                    changed = true;
                }
                break;
            }
        }
    }

    if changed {
        formatted.push_str(&source[last_match.to_usize()..]);
        Ok(MarkdownResult::Formatted(formatted))
    } else {
        Ok(MarkdownResult::Unchanged)
    }
}

fn relocate_parse_error(
    error: &mut FormatModuleError,
    original: &str,
    extracted: &str,
    original_start: TextSize,
) {
    if let FormatModuleError::ParseError(error) = error {
        error.location = TextRange::new(
            original_start + map_offset_to_original(original, extracted, error.location.start()),
            original_start + map_offset_to_original(original, extracted, error.location.end()),
        );
    }
}

fn map_offset_to_original(original: &str, extracted: &str, offset: TextSize) -> TextSize {
    original
        .universal_newlines()
        .zip(extracted.universal_newlines())
        .find(|(_, extracted_line)| offset < extracted_line.full_end())
        .map_or(original.text_len(), |(original_line, extracted_line)| {
            original_line.full_end() - (extracted_line.full_end() - offset)
        })
}

fn format_pycon_block(
    source: &str,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> Result<Option<String>, FormatModuleError> {
    static FIRST_LINE: &str = ">>> ";
    static CONTINUATION: &str = "... ";
    static CONTINUATION_BLANK: &str = "...";

    let offset = FIRST_LINE.text_len();
    let mut changed = false;
    let mut result = String::with_capacity(source.len());
    let mut unformatted = String::with_capacity(source.len());
    let mut last_match = TextSize::new(0);
    let mut lines = source.universal_newlines().peekable();

    while let Some(line) = lines.next() {
        unformatted.clear();
        if line.starts_with(FIRST_LINE) {
            let start = line.start();
            let mut end = line.full_end();
            unformatted.push_str(&source[TextRange::new(line.start() + offset, line.full_end())]);
            while let Some(next_line) = lines.next_if(|line| line.starts_with(CONTINUATION_BLANK)) {
                end = next_line.full_end();
                let start = if next_line.trim_end() == CONTINUATION_BLANK {
                    next_line.end()
                } else {
                    next_line.start() + offset
                };
                unformatted.push_str(&source[TextRange::new(start, end)]);
            }
            let options = settings.to_format_options(PySourceType::Python, &unformatted, path);
            let formatted = match format_module_source(&unformatted, options) {
                Ok(formatted) => formatted.into_code(),
                Err(mut error) if is_code_block_errors_enabled(settings) => {
                    let statement = &source[TextRange::new(start, end)];
                    relocate_parse_error(&mut error, statement, &unformatted, start);
                    return Err(error);
                }
                Err(_) => continue,
            };

            if formatted.len() != unformatted.len() || formatted != unformatted {
                result.push_str(&source[TextRange::new(last_match, start)]);
                for (idx, line) in formatted.universal_newlines().enumerate() {
                    result.push_str(if idx == 0 {
                        FIRST_LINE
                    } else if line.is_empty() {
                        CONTINUATION_BLANK
                    } else {
                        CONTINUATION
                    });
                    result.push_str(&formatted[line.full_range()]);
                }
                last_match = end;
                changed = true;
            }
        }
    }

    if changed {
        result.push_str(&source[last_match.to_usize()..]);
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_linter::settings::types::{ExtensionMapping, ExtensionPair, Language};
    use ruff_python_formatter::{FormatModuleError, PreviewMode};
    use ruff_source_file::LineIndex;
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

    fn preview_settings() -> FormatterSettings {
        FormatterSettings {
            preview: PreviewMode::Enabled,
            ..FormatterSettings::default()
        }
    }

    /// Formats `code` in preview mode and renders the resulting parse error as
    /// `line:column: message (text at the error location)`, where the line and column are
    /// relative to the Markdown document.
    fn preview_syntax_error(code: &str) -> String {
        match format_code_blocks(code, None, &preview_settings()) {
            Err(FormatModuleError::ParseError(error)) => {
                let location =
                    LineIndex::from_source_text(code).line_column(error.location.start(), code);
                format!(
                    "{}:{}: {} ({:?})",
                    location.line, location.column, error.error, &code[error.location]
                )
            }
            result => panic!("Expected a parse error, got {result:?}"),
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
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
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
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
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
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_syntax_error_preview() {
        // The error fails the whole document, so the first code block isn't formatted either.
        let code = r#"
This is poorly formatted code:

```py
print( "hello" )
```

This is invalid code:

```py
print "hello"
```
        "#;
        assert_snapshot!(preview_syntax_error(code), @r#"11:7: Simple statements must be separated by newlines or semicolons ("\"hello\"")"#);
    }

    #[test]
    fn format_code_blocks_syntax_error_indented_preview() {
        let code = r#"
- This is invalid code in a list item:

    ```py
    if True:
        print "hello"
    ```
        "#;
        assert_snapshot!(preview_syntax_error(code), @r#"6:15: Simple statements must be separated by newlines or semicolons ("\"hello\"")"#);
    }

    #[test]
    fn format_code_blocks_syntax_error_end_of_block_preview() {
        let code = r#"
```py
def foo():
```
        "#;
        assert_snapshot!(preview_syntax_error(code), @r#"3:11: Expected an indented block after function definition ("\n")"#);
    }

    #[test]
    fn format_code_blocks_syntax_error_ignore_ruff_off_preview() {
        let code = r#"
<!-- fmt:off -->
```py
print "hello"
```
<!-- fmt:on -->
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &preview_settings()).unwrap(),
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
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @"Unchanged");
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
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_tildes() {
        let code = r#"
~~~py
print( 'hello' )
~~~
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @r#"

        ~~~py
        print("hello")
        ~~~
        "#);
    }

    #[test]
    fn format_code_blocks_long_fence() {
        let code = r#"
````py
print( 'hello' )
````
~~~~~py
print( 'hello' )
~~~~~
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @r#"

        ````py
        print("hello")
        ````
        ~~~~~py
        print("hello")
        ~~~~~
        "#);
    }

    #[test]
    fn format_code_blocks_longer_closing_fence() {
        let code = r#"
```py
print( 'hello' )
````
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @r#"

        ```py
        print("hello")
        ````
        "#);
    }

    #[test]
    fn format_code_blocks_invalid_closing_fence_info() {
        let code = r#"
```py
print( 'hello' )
```not_a_close
print( 'world' )
```
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_invalid_closing_fence_form_feed() {
        let code = "```py\nprint( 'hello' )\n```\x0C\nprint( 'world' )\n```\n";
        assert_eq!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            MarkdownResult::Unchanged
        );
    }

    #[test]
    fn format_code_blocks_nested() {
        let code = r#"
````markdown
```py
print( 'hello' )
```
````
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()).unwrap(),
            @"Unchanged");
    }

    #[test]
    fn format_code_blocks_ignore_blackendocs_off() {
        let code = r#"
```py
print( 'hello' )
```

<!-- blacken-docs:off -->
```py
print( 'hello' )
```
<!-- blacken-docs:on -->

```py
print( 'hello' )
```
        "#;
        assert_snapshot!(format_code_blocks(
            code,
            None,
            &FormatterSettings::default()
        ).unwrap(), @r#"

        ```py
        print("hello")
        ```

        <!-- blacken-docs:off -->
        ```py
        print( 'hello' )
        ```
        <!-- blacken-docs:on -->

        ```py
        print("hello")
        ```
        "#);
    }

    #[test]
    fn format_code_blocks_ignore_ruff_off() {
        let code = r#"
```py
print( 'hello' )
```

<!-- fmt:off -->
```py
print( 'hello' )
```
<!-- fmt:on -->

```py
print( 'hello' )
```
        "#;
        assert_snapshot!(format_code_blocks(
            code,
            None,
            &FormatterSettings::default()
        ).unwrap(), @r#"

        ```py
        print("hello")
        ```

        <!-- fmt:off -->
        ```py
        print( 'hello' )
        ```
        <!-- fmt:on -->

        ```py
        print("hello")
        ```
        "#);
    }

    #[test]
    fn format_code_blocks_ignore_to_end() {
        let code = r#"
<!-- fmt:off -->
```py
print( 'hello' )
```

```py
print( 'hello' )
```
        "#;
        assert_snapshot!(format_code_blocks(
            code,
            None,
            &FormatterSettings::default()
        ).unwrap(), @"Unchanged");
    }

    #[test]
    fn format_code_blocks_extension_mapping() {
        // format "py" mapped as "pyi" instead
        let code = r#"
```py
def foo(): ...
def bar(): ...
```
        "#;
        let mapping = ExtensionMapping::from_iter([ExtensionPair {
            extension: "py".to_string(),
            language: Language::Pyi,
        }]);
        assert_snapshot!(format_code_blocks(
            code,
            None,
            &FormatterSettings {
                extension: mapping,
                ..Default::default()
            }
        ).unwrap(), @"Unchanged");
    }

    #[test]
    fn format_code_blocks_quarto() {
        let code = r#"
```{py}
print( 'hello' )
```

~~~{pyi}
def foo(): ...


def bar(): ...
~~~
        "#;
        assert_snapshot!(format_code_blocks(code, None, &FormatterSettings::default()).unwrap(), @r#"

        ```{py}
        print("hello")
        ```

        ~~~{pyi}
        def foo(): ...
        def bar(): ...
        ~~~
        "#);
    }

    #[test]
    fn format_code_blocks_python_console() {
        let code = r#"
```pycon
>>> print( 'hello there' )
hello there
>>> def foo(): pass
>>> def bar():
...   print( 'thing1', "thing2", )
...
... bar()
...
thing1 thing2
```
        "#;
        assert_snapshot!(format_code_blocks(code, None, &FormatterSettings::default()).unwrap(), @r#"

        ```pycon
        >>> print("hello there")
        hello there
        >>> def foo():
        ...     pass
        >>> def bar():
        ...     print(
        ...         "thing1",
        ...         "thing2",
        ...     )
        ...
        ...
        ... bar()
        thing1 thing2
        ```
        "#);
    }

    #[test]
    fn format_code_blocks_python_console_syntax_error() {
        // Outside of preview, only the invalid statement is skipped.
        let code = r#"
```pycon
>>> print( 'hello' )
hello
>>> print "hello"
```
        "#;
        assert_snapshot!(format_code_blocks(code, None, &FormatterSettings::default()).unwrap(), @r#"

        ```pycon
        >>> print("hello")
        hello
        >>> print "hello"
        ```
        "#);
    }

    #[test]
    fn format_code_blocks_python_console_syntax_error_preview() {
        let code = r#"
- This is an invalid console session in a list item:

    ```pycon
    >>> print( 'hello' )
    hello
    >>> def foo():
    ...     print "hello"
    ```
        "#;
        assert_snapshot!(preview_syntax_error(code), @r#"8:19: Simple statements must be separated by newlines or semicolons ("\"hello\"")"#);
    }
}
