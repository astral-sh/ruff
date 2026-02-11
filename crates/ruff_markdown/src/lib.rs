use std::{path::Path, sync::LazyLock};

use regex::Regex;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::format_module_source;
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

pub fn format_code_blocks(
    source: &str,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> MarkdownResult {
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
            let start = lines.peek().map(Line::start).unwrap_or_default();

            // Consume lines until reaching the matching/ending code fence
            for code_line in lines.by_ref() {
                let Some((_, [_, closing_fence, _, _])) = MARKDOWN_CODE_FENCE
                    .captures(&code_line)
                    .map(|cap| cap.extract())
                else {
                    continue;
                };

                if closing_fence != opening_fence {
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
                let unformatted_code = dedent(&source[TextRange::new(start, end)]);

                let formatted_code = match language.as_str() {
                    "python" | "py" | "python3" | "py3" | "pyi" => {
                        let options =
                            settings.to_format_options(py_source_type, &unformatted_code, path);
                        // Using `Printed::into_code` requires adding `ruff_formatter` as a direct
                        // dependency, and I suspect that Rust can optimize the closure away regardless.
                        #[expect(clippy::redundant_closure_for_method_calls)]
                        format_module_source(&unformatted_code, options)
                            .map(|formatted| formatted.into_code())
                            .ok()
                    }
                    "pycon" => format_pycon_block(&unformatted_code, path, settings),
                    _ => None,
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
        MarkdownResult::Formatted(formatted)
    } else {
        MarkdownResult::Unchanged
    }
}

fn format_pycon_block(
    source: &str,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> Option<String> {
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
            // Using `Printed::into_code` requires adding `ruff_formatter` as a direct
            // dependency, and I suspect that Rust can optimize the closure away regardless.
            #[expect(clippy::redundant_closure_for_method_calls)]
            let Ok(formatted) =
                format_module_source(&unformatted, options).map(|formatted| formatted.into_code())
            else {
                continue;
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
        Some(result)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_linter::settings::types::{ExtensionMapping, ExtensionPair, Language};
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
            format_code_blocks(code, None, &FormatterSettings::default()),
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
            format_code_blocks(code, None, &FormatterSettings::default()),
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
            format_code_blocks(code, None, &FormatterSettings::default()),
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
    fn format_code_blocks_nested() {
        let code = r#"
````markdown
```py
print( 'hello' )
```
````
        "#;
        assert_snapshot!(
            format_code_blocks(code, None, &FormatterSettings::default()),
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
        ), @r#"
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
        ), @r#"
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
        ), @"Unchanged");
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
        ), @"Unchanged");
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
        assert_snapshot!(format_code_blocks(code, None, &FormatterSettings::default()), @r#"
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
        assert_snapshot!(format_code_blocks(code, None, &FormatterSettings::default()), @r#"

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
}
