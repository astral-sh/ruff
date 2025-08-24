use std::borrow::Cow;
use std::num::NonZeroUsize;

use anstyle::Style;
use similar::{ChangeTag, TextDiff};

use ruff_annotate_snippets::Renderer as AnnotateRenderer;
use ruff_diagnostics::{Applicability, Fix};
use ruff_source_file::OneIndexed;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::diagnostic::render::{FileResolver, Resolved};
use crate::diagnostic::stylesheet::{DiagnosticStylesheet, fmt_styled};
use crate::diagnostic::{Diagnostic, DiagnosticSource, DisplayDiagnosticConfig};

pub(super) struct FullRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> FullRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        let stylesheet = if self.config.color {
            DiagnosticStylesheet::styled()
        } else {
            DiagnosticStylesheet::plain()
        };

        let mut renderer = if self.config.color {
            AnnotateRenderer::styled()
        } else {
            AnnotateRenderer::plain()
        }
        .cut_indicator("…");

        renderer = renderer
            .error(stylesheet.error)
            .warning(stylesheet.warning)
            .info(stylesheet.info)
            .note(stylesheet.note)
            .help(stylesheet.help)
            .line_no(stylesheet.line_no)
            .emphasis(stylesheet.emphasis)
            .none(stylesheet.none);

        for diag in diagnostics {
            let resolved = Resolved::new(self.resolver, diag, self.config);
            let renderable = resolved.to_renderable(self.config.context);
            for diag in renderable.diagnostics.iter() {
                writeln!(f, "{}", renderer.render(diag.to_annotate()))?;
            }
            writeln!(f)?;

            if self.config.show_fix_diff {
                if let Some(diff) = Diff::from_diagnostic(diag, &stylesheet, self.resolver) {
                    writeln!(f, "{diff}")?;
                }
            }
        }

        Ok(())
    }
}

/// Renders a diff that shows the code fixes.
///
/// The implementation isn't fully fledged out and only used by tests. Before using in production, try
/// * Improve layout
/// * Replace tabs with spaces for a consistent experience across terminals
/// * Replace zero-width whitespaces
/// * Print a simpler diff if only a single line has changed
/// * Compute the diff from the `Edit` because diff calculation is expensive.
struct Diff<'a> {
    fix: &'a Fix,
    diagnostic_source: DiagnosticSource,
    stylesheet: &'a DiagnosticStylesheet,
}

impl<'a> Diff<'a> {
    fn from_diagnostic(
        diagnostic: &'a Diagnostic,
        stylesheet: &'a DiagnosticStylesheet,
        resolver: &'a dyn FileResolver,
    ) -> Option<Diff<'a>> {
        Some(Diff {
            fix: diagnostic.fix()?,
            diagnostic_source: diagnostic
                .primary_span_ref()?
                .file
                .diagnostic_source(resolver),
            stylesheet,
        })
    }
}

impl std::fmt::Display for Diff<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source_code = self.diagnostic_source.as_source_code();
        let source_text = source_code.text();

        // TODO(dhruvmanila): Add support for Notebook cells once it's user-facing
        let mut output = String::with_capacity(source_text.len());
        let mut last_end = TextSize::default();

        for edit in self.fix.edits() {
            output.push_str(source_code.slice(TextRange::new(last_end, edit.start())));
            output.push_str(edit.content().unwrap_or_default());
            last_end = edit.end();
        }

        output.push_str(&source_text[usize::from(last_end)..]);

        let diff = TextDiff::from_lines(source_text, &output);

        let message = match self.fix.applicability() {
            // TODO(zanieb): Adjust this messaging once it's user-facing
            Applicability::Safe => "Safe fix",
            Applicability::Unsafe => "Unsafe fix",
            Applicability::DisplayOnly => "Display-only fix",
        };

        // TODO(brent) `stylesheet.separator` is cyan rather than blue, as we had before. I think
        // we're getting rid of this soon anyway, so I didn't think it was worth adding another
        // style to the stylesheet temporarily. The color doesn't appear at all in the snapshot
        // tests, which is the only place these are currently used.
        writeln!(f, "ℹ {}", fmt_styled(message, self.stylesheet.separator))?;

        let (largest_old, largest_new) = diff
            .ops()
            .last()
            .map(|op| (op.old_range().start, op.new_range().start))
            .unwrap_or_default();

        let digit_with = OneIndexed::from_zero_indexed(largest_new.max(largest_old)).digits();

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                writeln!(f, "{:-^1$}", "-", 80)?;
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };

                    let line_style = LineStyle::from(change.tag(), self.stylesheet);

                    let old_index = change.old_index().map(OneIndexed::from_zero_indexed);
                    let new_index = change.new_index().map(OneIndexed::from_zero_indexed);

                    write!(
                        f,
                        "{} {} |{}",
                        Line {
                            index: old_index,
                            width: digit_with
                        },
                        Line {
                            index: new_index,
                            width: digit_with
                        },
                        fmt_styled(line_style.apply_to(sign), self.stylesheet.emphasis),
                    )?;

                    for (emphasized, value) in change.iter_strings_lossy() {
                        let value = show_nonprinting(&value);
                        if emphasized {
                            write!(
                                f,
                                "{}",
                                fmt_styled(line_style.apply_to(&value), self.stylesheet.underline)
                            )?;
                        } else {
                            write!(f, "{}", line_style.apply_to(&value))?;
                        }
                    }
                    if change.missing_newline() {
                        writeln!(f)?;
                    }
                }
            }
        }

        Ok(())
    }
}

struct LineStyle {
    style: Style,
}

impl LineStyle {
    fn apply_to(&self, input: &str) -> impl std::fmt::Display {
        fmt_styled(input, self.style)
    }

    fn from(value: ChangeTag, stylesheet: &DiagnosticStylesheet) -> LineStyle {
        match value {
            ChangeTag::Equal => LineStyle {
                style: stylesheet.none,
            },
            ChangeTag::Delete => LineStyle {
                style: stylesheet.deletion,
            },
            ChangeTag::Insert => LineStyle {
                style: stylesheet.insertion,
            },
        }
    }
}

struct Line {
    index: Option<OneIndexed>,
    width: NonZeroUsize,
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.index {
            None => {
                for _ in 0..self.width.get() {
                    f.write_str(" ")?;
                }
                Ok(())
            }
            Some(idx) => write!(f, "{:<width$}", idx, width = self.width.get()),
        }
    }
}

fn show_nonprinting(s: &str) -> Cow<'_, str> {
    if s.find(['\x07', '\x08', '\x1b', '\x7f']).is_some() {
        Cow::Owned(
            s.replace('\x07', "␇")
                .replace('\x08', "␈")
                .replace('\x1b', "␛")
                .replace('\x7f', "␡"),
        )
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::Applicability;
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use crate::diagnostic::{
        Annotation, DiagnosticFormat, Severity,
        render::tests::{
            NOTEBOOK, TestEnvironment, create_diagnostics, create_notebook_diagnostics,
            create_syntax_error_diagnostics,
        },
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Full);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r#"
        error[unused-import]: `os` imported but unused
         --> fib.py:1:8
          |
        1 | import os
          |        ^^
          |
        help: Remove unused import: `os`

        error[unused-variable]: Local variable `x` is assigned to but never used
         --> fib.py:6:5
          |
        4 | def fibonacci(n):
        5 |     """Compute the nth number in the Fibonacci sequence."""
        6 |     x = 1
          |     ^
        7 |     if n == 0:
        8 |         return 0
          |
        help: Remove assignment to unused variable `x`

        error[undefined-name]: Undefined name `a`
         --> undef.py:1:4
          |
        1 | if a == 1: pass
          |    ^
          |
        "#);
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Full);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        error[invalid-syntax]: Expected one or more symbol names after import
         --> syntax_errors.py:1:15
          |
        1 | from os import
          |               ^
        2 |
        3 | if call(foo
          |

        error[invalid-syntax]: Expected ')', found newline
         --> syntax_errors.py:3:12
          |
        1 | from os import
        2 |
        3 | if call(foo
          |            ^
        4 |     def bar():
        5 |         pass
          |
        ");
    }

    #[test]
    fn hide_severity_output() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Full);
        env.hide_severity(true);
        env.fix_applicability(Applicability::DisplayOnly);

        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r#"
        F401 [*] `os` imported but unused
         --> fib.py:1:8
          |
        1 | import os
          |        ^^
          |
        help: Remove unused import: `os`

        F841 [*] Local variable `x` is assigned to but never used
         --> fib.py:6:5
          |
        4 | def fibonacci(n):
        5 |     """Compute the nth number in the Fibonacci sequence."""
        6 |     x = 1
          |     ^
        7 |     if n == 0:
        8 |         return 0
          |
        help: Remove assignment to unused variable `x`

        F821 Undefined name `a`
         --> undef.py:1:4
          |
        1 | if a == 1: pass
          |    ^
          |
        "#);
    }

    #[test]
    fn hide_severity_syntax_errors() {
        let (mut env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Full);
        env.hide_severity(true);

        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        invalid-syntax: Expected one or more symbol names after import
         --> syntax_errors.py:1:15
          |
        1 | from os import
          |               ^
        2 |
        3 | if call(foo
          |

        invalid-syntax: Expected ')', found newline
         --> syntax_errors.py:3:12
          |
        1 | from os import
        2 |
        3 | if call(foo
          |            ^
        4 |     def bar():
        5 |         pass
          |
        ");
    }

    /// Check that the new `full` rendering code in `ruff_db` handles cases fixed by commit c9b99e4.
    ///
    /// For example, without the fix, we get diagnostics like this:
    ///
    /// ```
    /// error[no-indented-block]: Expected an indented block
    ///  --> example.py:3:1
    ///   |
    /// 2 | if False:
    ///   |          ^
    /// 3 | print()
    ///   |
    ///  ```
    ///
    /// where the caret points to the end of the previous line instead of the start of the next.
    #[test]
    fn empty_span_after_line_terminator() {
        let mut env = TestEnvironment::new();
        env.add(
            "example.py",
            r#"
if False:
print()
"#,
        );
        env.format(DiagnosticFormat::Full);

        let diagnostic = env
            .builder(
                "no-indented-block",
                Severity::Error,
                "Expected an indented block",
            )
            .primary("example.py", "3:0", "3:0", "")
            .build();

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[no-indented-block]: Expected an indented block
         --> example.py:3:1
          |
        2 | if False:
        3 | print()
          | ^
          |
        ");
    }

    /// Check that the new `full` rendering code in `ruff_db` handles cases fixed by commit 2922490.
    ///
    /// For example, without the fix, we get diagnostics like this:
    ///
    /// ```
    /// error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1a" instead
    ///  --> example.py:1:25
    ///   |
    /// 1 | nested_fstrings = f'␈{f'{f'␛'}'}'
    ///   |                       ^
    ///   |
    ///  ```
    ///
    /// where the caret points to the `f` in the f-string instead of the start of the invalid
    /// character (`^Z`).
    #[test]
    fn unprintable_characters() {
        let mut env = TestEnvironment::new();
        env.add("example.py", "nested_fstrings = f'{f'{f''}'}'");
        env.format(DiagnosticFormat::Full);

        let diagnostic = env
            .builder(
                "invalid-character-sub",
                Severity::Error,
                r#"Invalid unescaped character SUB, use "\x1a" instead"#,
            )
            .primary("example.py", "1:24", "1:24", "")
            .build();

        insta::assert_snapshot!(env.render(&diagnostic), @r#"
        error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1a" instead
         --> example.py:1:25
          |
        1 | nested_fstrings = f'␈{f'{f'␛'}'}'
          |                         ^
          |
        "#);
    }

    #[test]
    fn multiple_unprintable_characters() -> std::io::Result<()> {
        let mut env = TestEnvironment::new();
        env.add("example.py", "");
        env.format(DiagnosticFormat::Full);

        let diagnostic = env
            .builder(
                "invalid-character-sub",
                Severity::Error,
                r#"Invalid unescaped character SUB, use "\x1a" instead"#,
            )
            .primary("example.py", "1:1", "1:1", "")
            .build();

        insta::assert_snapshot!(env.render(&diagnostic), @r#"
        error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1a" instead
         --> example.py:1:2
          |
        1 | ␈␛
          |  ^
          |
        "#);

        Ok(())
    }

    /// Ensure that the header column matches the column in the user's input, even if we've replaced
    /// tabs with spaces for rendering purposes.
    #[test]
    fn tab_replacement() {
        let mut env = TestEnvironment::new();
        env.add("example.py", "def foo():\n\treturn 1");
        env.format(DiagnosticFormat::Full);

        let diagnostic = env.err().primary("example.py", "2:1", "2:9", "").build();

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
         --> example.py:2:2
          |
        1 | def foo():
        2 |     return 1
          |     ^^^^^^^^
          |
        ");
    }

    /// For file-level diagnostics, we expect to see the header line with the diagnostic information
    /// and the `-->` line with the file information but no lines of source code.
    #[test]
    fn file_level() {
        let mut env = TestEnvironment::new();
        env.add("example.py", "");
        env.format(DiagnosticFormat::Full);

        let mut diagnostic = env.err().build();
        let span = env.path("example.py").with_range(TextRange::default());
        let mut annotation = Annotation::primary(span);
        annotation.set_file_level(true);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
        --> example.py:1:1
        ");
    }

    /// Check that ranges in notebooks are remapped relative to the cells.
    #[test]
    fn notebook_output() {
        let (env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        error[unused-import][*]: `os` imported but unused
         --> notebook.ipynb:cell 1:2:8
          |
        1 | # cell 1
        2 | import os
          |        ^^
          |
        help: Remove unused import: `os`

        error[unused-import][*]: `math` imported but unused
         --> notebook.ipynb:cell 2:2:8
          |
        1 | # cell 2
        2 | import math
          |        ^^^^
        3 |
        4 | print('hello world')
          |
        help: Remove unused import: `math`

        error[unused-variable]: Local variable `x` is assigned to but never used
         --> notebook.ipynb:cell 3:4:5
          |
        2 | def foo():
        3 |     print()
        4 |     x = 1
          |     ^
          |
        help: Remove assignment to unused variable `x`
        ");
    }

    /// Check notebook handling for multiple annotations in a single diagnostic that span cells.
    #[test]
    fn notebook_output_multiple_annotations() {
        let mut env = TestEnvironment::new();
        env.add("notebook.ipynb", NOTEBOOK);

        let diagnostics = vec![
            // adjacent context windows
            env.builder("unused-import", Severity::Error, "`os` imported but unused")
                .primary("notebook.ipynb", "2:7", "2:9", "")
                .secondary("notebook.ipynb", "4:7", "4:11", "second cell")
                .help("Remove unused import: `os`")
                .build(),
            // non-adjacent context windows
            env.builder("unused-import", Severity::Error, "`os` imported but unused")
                .primary("notebook.ipynb", "2:7", "2:9", "")
                .secondary("notebook.ipynb", "10:4", "10:5", "second cell")
                .help("Remove unused import: `os`")
                .build(),
            // adjacent context windows in the same cell
            env.err()
                .primary("notebook.ipynb", "4:7", "4:11", "second cell")
                .secondary("notebook.ipynb", "6:0", "6:5", "print statement")
                .help("Remove `print` statement")
                .build(),
        ];

        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        error[unused-import]: `os` imported but unused
         --> notebook.ipynb:cell 1:2:8
          |
        1 | # cell 1
        2 | import os
          |        ^^
          |
         ::: notebook.ipynb:cell 2:2:8
          |
        1 | # cell 2
        2 | import math
          |        ---- second cell
        3 |
        4 | print('hello world')
          |
        help: Remove unused import: `os`

        error[unused-import]: `os` imported but unused
         --> notebook.ipynb:cell 1:2:8
          |
        1 | # cell 1
        2 | import os
          |        ^^
          |
         ::: notebook.ipynb:cell 3:4:5
          |
        2 | def foo():
        3 |     print()
        4 |     x = 1
          |     - second cell
          |
        help: Remove unused import: `os`

        error[test-diagnostic]: main diagnostic message
         --> notebook.ipynb:cell 2:2:8
          |
        1 | # cell 2
        2 | import math
          |        ^^^^ second cell
        3 |
        4 | print('hello world')
          | ----- print statement
          |
        help: Remove `print` statement
        ");
    }

    /// Carriage return (`\r`) is a valid line-ending in Python, so we should normalize this to a
    /// line feed (`\n`) for rendering. Otherwise we report a single long line for this case.
    #[test]
    fn normalize_carriage_return() {
        let mut env = TestEnvironment::new();
        env.add(
            "example.py",
            "# Keep parenthesis around preserved CR\rint(-\r    1)\rint(+\r    1)",
        );
        env.format(DiagnosticFormat::Full);

        let mut diagnostic = env.err().build();
        let span = env
            .path("example.py")
            .with_range(TextRange::at(TextSize::new(39), TextSize::new(0)));
        let annotation = Annotation::primary(span);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
         --> example.py:2:1
          |
        1 | # Keep parenthesis around preserved CR
        2 | int(-
          | ^
        3 |     1)
        4 | int(+
          |
        ");
    }

    /// Without stripping the BOM, we report an error in column 2, unlike Ruff.
    #[test]
    fn strip_bom() {
        let mut env = TestEnvironment::new();
        env.add("example.py", "\u{feff}import foo");
        env.format(DiagnosticFormat::Full);

        let mut diagnostic = env.err().build();
        let span = env
            .path("example.py")
            .with_range(TextRange::at(TextSize::new(3), TextSize::new(0)));
        let annotation = Annotation::primary(span);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
         --> example.py:1:1
          |
        1 | import foo
          | ^
          |
        ");
    }

    #[test]
    fn bom_with_default_range() {
        let mut env = TestEnvironment::new();
        env.add("example.py", "\u{feff}import foo");
        env.format(DiagnosticFormat::Full);

        let mut diagnostic = env.err().build();
        let span = env.path("example.py").with_range(TextRange::default());
        let annotation = Annotation::primary(span);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
         --> example.py:1:1
          |
        1 | import foo
          | ^
          |
        ");
    }

    /// We previously rendered this correctly, but the header was falling back to 1:1 for ranges
    /// pointing to the final newline in a file. Like Ruff, we now use the offset of the first
    /// character in the nonexistent final line in the header.
    #[test]
    fn end_of_file() {
        let mut env = TestEnvironment::new();
        let contents = "unexpected eof\n";
        env.add("example.py", contents);
        env.format(DiagnosticFormat::Full);

        let mut diagnostic = env.err().build();
        let span = env
            .path("example.py")
            .with_range(TextRange::at(contents.text_len(), TextSize::new(0)));
        let annotation = Annotation::primary(span);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
         --> example.py:2:1
          |
        1 | unexpected eof
          |               ^
          |
        ");
    }
}
