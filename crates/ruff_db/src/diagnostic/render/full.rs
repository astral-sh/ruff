use std::borrow::Cow;
use std::num::NonZeroUsize;

use similar::{ChangeTag, TextDiff};

use ruff_annotate_snippets::Renderer as AnnotateRenderer;
use ruff_diagnostics::{Applicability, Fix};
use ruff_notebook::NotebookIndex;
use ruff_source_file::OneIndexed;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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
            .none(stylesheet.none)
            .hyperlink(stylesheet.hyperlink);

        for diag in diagnostics {
            if self.config.is_canceled() {
                return Ok(());
            }

            let resolved = Resolved::new(self.resolver, diag, self.config);
            let renderable = resolved.to_renderable(self.config.context);
            for diag in renderable.diagnostics.iter() {
                writeln!(f, "{}", renderer.render(diag.to_annotate()))?;
            }

            if self.config.show_fix_diff
                && diag.has_applicable_fix(self.config)
                && let Some(diff) = Diff::from_diagnostic(diag, &stylesheet, self.resolver)
            {
                write!(f, "{diff}")?;
            }

            writeln!(f)?;
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
    notebook_index: Option<NotebookIndex>,
    stylesheet: &'a DiagnosticStylesheet,
}

impl<'a> Diff<'a> {
    fn from_diagnostic(
        diagnostic: &'a Diagnostic,
        stylesheet: &'a DiagnosticStylesheet,
        resolver: &'a dyn FileResolver,
    ) -> Option<Diff<'a>> {
        let file = &diagnostic.primary_span_ref()?.file;
        Some(Diff {
            fix: diagnostic.fix()?,
            diagnostic_source: file.diagnostic_source(resolver),
            notebook_index: resolver.notebook_index(file),
            stylesheet,
        })
    }
}

impl std::fmt::Display for Diff<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source_code = self.diagnostic_source.as_source_code();
        let source_text = source_code.text();

        // Partition the source code into end offsets for each cell. If `self.notebook_index` is
        // `None`, indicating a regular script file, all the lines will be in one "cell" under the
        // `None` key.
        let cells = if let Some(notebook_index) = &self.notebook_index {
            let mut last_cell_index = OneIndexed::MIN;
            let mut cells: Vec<(Option<OneIndexed>, TextSize)> = Vec::new();
            for cell in notebook_index.iter() {
                if cell.cell_index() != last_cell_index {
                    let offset = source_code.line_start(cell.start_row());
                    cells.push((Some(last_cell_index), offset));
                    last_cell_index = cell.cell_index();
                }
            }
            cells.push((Some(last_cell_index), source_text.text_len()));
            cells
        } else {
            vec![(None, source_text.text_len())]
        };

        let mut last_end = TextSize::ZERO;
        for (cell, offset) in cells {
            let range = TextRange::new(last_end, offset);
            last_end = offset;
            let input = source_code.slice(range);

            let mut output = String::with_capacity(input.len());
            let mut last_end = range.start();

            let mut applied = 0;
            for edit in self.fix.edits() {
                if range.contains_range(edit.range()) {
                    output.push_str(source_code.slice(TextRange::new(last_end, edit.start())));
                    output.push_str(edit.content().unwrap_or_default());
                    last_end = edit.end();
                    applied += 1;
                }
            }

            // No edits were applied, so there's no need to diff.
            if applied == 0 {
                continue;
            }

            output.push_str(&source_text[usize::from(last_end)..usize::from(range.end())]);

            let diff = TextDiff::from_lines(input, &output);

            let grouped_ops = diff.grouped_ops(3);

            // Find the new line number with the largest number of digits to align all of the line
            // number separators.
            let last_op = grouped_ops.last().and_then(|group| group.last());
            let largest_new = last_op.map(|op| op.new_range().end).unwrap_or_default();

            let digit_with = OneIndexed::new(largest_new).unwrap_or_default().digits();

            if let Some(cell) = cell {
                // Room for 1 digit, 1 space, 1 `|`, and 1 more following space. This centers the
                // three colons on the pipe.
                writeln!(f, "{:>1$} cell {cell}", ":::", digit_with.get() + 3)?;
            }

            for (idx, group) in grouped_ops.iter().enumerate() {
                if idx > 0 {
                    writeln!(f, "{:-^1$}", "-", 80)?;
                }
                for op in group {
                    for change in diff.iter_inline_changes(op) {
                        let (sign, style, line_no_style, index) = match change.tag() {
                            ChangeTag::Delete => (
                                "-",
                                self.stylesheet.deletion,
                                self.stylesheet.deletion_line_no,
                                None,
                            ),
                            ChangeTag::Insert => (
                                "+",
                                self.stylesheet.insertion,
                                self.stylesheet.insertion_line_no,
                                change.new_index(),
                            ),
                            ChangeTag::Equal => (
                                "|",
                                self.stylesheet.none,
                                self.stylesheet.line_no,
                                change.new_index(),
                            ),
                        };

                        let line = Line {
                            index: index.map(OneIndexed::from_zero_indexed),
                            width: digit_with,
                        };

                        write!(
                            f,
                            "{line} {sign} ",
                            line = fmt_styled(line, self.stylesheet.line_no),
                            sign = fmt_styled(sign, line_no_style),
                        )?;

                        for (emphasized, value) in change.iter_strings_lossy() {
                            let value = show_nonprinting(&value);
                            let styled = fmt_styled(value, style);
                            if emphasized {
                                write!(f, "{}", fmt_styled(styled, self.stylesheet.emphasis))?;
                            } else {
                                write!(f, "{styled}")?;
                            }
                        }
                        if change.missing_newline() {
                            writeln!(f)?;
                        }
                    }
                }
            }
        }

        match self.fix.applicability() {
            Applicability::Safe => {}
            Applicability::Unsafe => {
                writeln!(
                    f,
                    "{note}: {msg}",
                    note = fmt_styled("note", self.stylesheet.warning),
                    msg = fmt_styled(
                        "This is an unsafe fix and may change runtime behavior",
                        self.stylesheet.emphasis
                    )
                )?;
            }
            Applicability::DisplayOnly => {
                // Note that this is still only used in tests. There's no `--display-only-fixes`
                // analog to `--unsafe-fixes` for users to activate this or see the styling.
                writeln!(
                    f,
                    "{note}: {msg}",
                    note = fmt_styled("note", self.stylesheet.error),
                    msg = fmt_styled(
                        "This is a display-only fix and is likely to be incorrect",
                        self.stylesheet.emphasis
                    )
                )?;
            }
        }

        Ok(())
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
    use ruff_diagnostics::{Applicability, Edit, Fix};
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
        env.show_fix_status(true);
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
        annotation.hide_snippet(true);
        diagnostic.annotate(annotation);

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic]: main diagnostic message
        --> example.py:1:1
        ");
    }

    /// Check that ranges in notebooks are remapped relative to the cells.
    #[test]
    fn notebook_output() {
        let (mut env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
        env.show_fix_status(true);
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

    /// Test that we remap notebook cell line numbers in the diff as well as the main diagnostic.
    #[test]
    fn notebook_output_with_diff() {
        let (mut env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
        env.show_fix_diff(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);

        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn notebook_output_with_diff_spanning_cells() {
        let (mut env, mut diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
        env.show_fix_diff(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);

        // Move all of the edits from the later diagnostics to the first diagnostic to simulate a
        // single diagnostic with edits in different cells.
        let mut diagnostic = diagnostics.swap_remove(0);
        let fix = diagnostic.fix_mut().unwrap();
        let mut edits = fix.edits().to_vec();
        for diag in diagnostics {
            edits.extend_from_slice(diag.fix().unwrap().edits());
        }
        *fix = Fix::unsafe_edits(edits.remove(0), edits);

        insta::assert_snapshot!(env.render(&diagnostic));
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

    /// Test that we handle the width calculation for the line number correctly even for context
    /// lines at the end of a diff. For example, we want it to render like this:
    ///
    /// ```
    /// 8  |
    /// 9  |
    /// 10 |
    /// ```
    ///
    /// and not like this:
    ///
    /// ```
    /// 8 |
    /// 9 |
    /// 10 |
    /// ```
    #[test]
    fn longer_line_number_end_of_context() {
        let mut env = TestEnvironment::new();
        let contents = "\
line 1
line 2
line 3
line 4
line 5
line 6
line 7
line 8
line 9
line 10
        ";
        env.add("example.py", contents);
        env.format(DiagnosticFormat::Full);
        env.show_fix_diff(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);

        let mut diagnostic = env.err().primary("example.py", "3", "3", "label").build();
        diagnostic.help("Start of diff:");
        let target = "line 7";
        let line9 = contents.find(target).unwrap();
        let range = TextRange::at(TextSize::try_from(line9).unwrap(), target.text_len());
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            format!("fixed {target}"),
            range,
        )));

        insta::assert_snapshot!(env.render(&diagnostic), @r"
        error[test-diagnostic][*]: main diagnostic message
         --> example.py:3:1
          |
        1 | line 1
        2 | line 2
        3 | line 3
          | ^^^^^^ label
        4 | line 4
        5 | line 5
          |
        help: Start of diff:
        4  | line 4
        5  | line 5
        6  | line 6
           - line 7
        7  + fixed line 7
        8  | line 8
        9  | line 9
        10 | line 10
        note: This is an unsafe fix and may change runtime behavior
        ");
    }
}
