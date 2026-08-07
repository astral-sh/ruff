use annotate_snippets::{
    Group as AnnotateGroup, Level as AnnotateLevel, Patch as AnnotatePatch,
    Renderer as AnnotateRenderer, Snippet as AnnotateSnippet,
};
use ruff_diagnostics::{Applicability, Fix};
use ruff_notebook::NotebookIndex;
use ruff_source_file::OneIndexed;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::diagnostic::render::{FileResolver, Resolved};
use crate::diagnostic::stylesheet::DiagnosticStylesheet;
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
            DiagnosticStylesheet::styled().hyperlinks(self.config.hyperlinks)
        } else {
            DiagnosticStylesheet::plain()
        };

        let mut renderer = if self.config.color {
            AnnotateRenderer::styled()
        } else {
            AnnotateRenderer::plain()
        }
        .cut_indicator("…")
        .anonymized_line_numbers(self.config.anonymized_line_numbers);

        renderer = renderer
            .error(stylesheet.error)
            .warning(stylesheet.warning)
            .info(stylesheet.info)
            .note(stylesheet.note)
            .help(stylesheet.help)
            .line_num(stylesheet.line_no)
            .emphasis(stylesheet.emphasis)
            .addition(stylesheet.insertion)
            .removal(stylesheet.deletion)
            .none(stylesheet.none)
            .hyperlink(stylesheet.hyperlink);

        for diag in diagnostics {
            if self.config.is_canceled() {
                return Ok(());
            }

            let resolved = Resolved::new(self.resolver, diag, self.config);
            let renderable = resolved.to_renderable(self.config);
            let diff = diag
                .has_applicable_fix(self.config.fix_applicability())
                .then(|| Diff::from_diagnostic(diag, self.resolver))
                .flatten();

            for (index, diagnostic) in renderable.diagnostics.iter().enumerate() {
                let mut group = diagnostic.to_annotate();
                if index + 1 == renderable.diagnostics.len()
                    && let Some(diff) = diff.as_ref()
                {
                    group = diff.append_to(group);
                }

                let rendered = renderer.render(&[group]);
                writeln!(f, "{rendered}")?;
            }

            if let Some(diff) = diff {
                if let Some(applicability) = to_applicability_annotate(diff.fix) {
                    writeln!(f, "{}", renderer.render(&[applicability]))?;
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

/// Track a diff for showing the code fixes.
struct Diff<'a> {
    fix: &'a Fix,
    diagnostic_source: DiagnosticSource,
    notebook_index: Option<NotebookIndex>,
    fold: bool,
}

impl<'a> Diff<'a> {
    fn from_diagnostic(diagnostic: &'a Diagnostic, resolver: &'a dyn FileResolver) -> Option<Self> {
        let file = &diagnostic.primary_span_ref()?.file;
        Some(Self {
            fix: diagnostic.fix()?,
            diagnostic_source: file.diagnostic_source(resolver),
            notebook_index: resolver.notebook_index(file),
            fold: !diagnostic
                .inner
                .annotations
                .iter()
                .any(|annotation| annotation.is_primary && annotation.hide_snippet),
        })
    }

    fn append_to<'s>(&'s self, mut group: AnnotateGroup<'s>) -> AnnotateGroup<'s> {
        let source_code = self.diagnostic_source.as_source_code();
        let source_text = source_code.text();

        let cell_ranges = self.cell_ranges();

        for (cell_index, range) in cell_ranges {
            // For non-notebooks, construct and diff only the source surrounding the edits.
            let (range, line_num) = if cell_index.is_none()
                && let Some(first) = self.fix.edits().first()
                && let Some(last) = self.fix.edits().last()
            {
                let start_line = source_code
                    .line_index(first.start())
                    .saturating_sub(DIFF_CONTEXT_WINDOW);
                let last_source_line = source_code.line_index(source_text.text_len());
                let end_line = source_code
                    .line_index(last.end())
                    .saturating_add(DIFF_CONTEXT_WINDOW)
                    .min(last_source_line);

                (
                    TextRange::new(
                        source_code.line_start(start_line),
                        source_code.line_end(end_line),
                    ),
                    start_line.get(),
                )
            } else {
                (range, 0)
            };

            let edits = self
                .fix
                .edits()
                .iter()
                .filter(|edit| range.contains_range(edit.range()))
                .collect::<Vec<_>>();
            // No edits were applied, so there's no need to diff.
            if edits.is_empty() {
                continue;
            }

            let input = source_code.slice(range);

            let snippet = AnnotateSnippet::source(input)
                .line_start(line_num)
                .cell_index(cell_index)
                .fold(self.fold)
                .patches(edits.iter().map(|edit| {
                    let range = edit.range() - range.start();
                    AnnotatePatch::new(
                        range.start().to_usize()..range.end().to_usize(),
                        edit.content().unwrap_or_default(),
                    )
                }));
            group = group.element(snippet);
        }

        group
    }

    fn cell_ranges(&self) -> Vec<(Option<usize>, TextRange)> {
        let source_code = self.diagnostic_source.as_source_code();
        let source_text = source_code.text();

        let mut last_end = TextSize::ZERO;
        let Some(notebook_index) = self.notebook_index.as_ref() else {
            // a regular script file, all the lines will be in one "cell" under the `None` key
            let offset = source_text.text_len();
            let range = TextRange::new(last_end, offset);
            return vec![(None, range)];
        };

        // Partition the source code into end offsets for each cell.
        let mut last_cell_index = OneIndexed::MIN;
        let mut cells: Vec<(Option<usize>, TextRange)> = Vec::new();
        for cell in notebook_index.iter() {
            if cell.cell_index() != last_cell_index {
                let offset = source_code.line_start(cell.start_row());
                let range = TextRange::new(last_end, offset);
                cells.push((Some(last_cell_index.get()), range));
                last_end = offset;
                last_cell_index = cell.cell_index();
            }
        }
        let offset = source_text.text_len();
        let range = TextRange::new(last_end, offset);
        cells.push((Some(last_cell_index.get()), range));
        cells
    }
}

/// Limit diffs to a narrow range around each fix rather than diffing the whole file.
const DIFF_CONTEXT_WINDOW: usize = 3;

fn to_applicability_annotate(fix: &Fix) -> Option<AnnotateGroup<'static>> {
    let (level, message) = match fix.applicability() {
        Applicability::Safe => return None,
        Applicability::Unsafe => (
            AnnotateLevel::WARNING,
            "This is an unsafe fix and may change runtime behavior",
        ),
        Applicability::DisplayOnly => (
            // Note that this is still only used in tests. There's no `--display-only-fixes`
            // analog to `--unsafe-fixes` for users to activate this or see the styling.
            AnnotateLevel::ERROR,
            "This is a display-only fix and is likely to be incorrect",
        ),
    };
    let level = level.with_name("note");

    Some(AnnotateGroup::with_title(level.primary_title(message)))
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
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r###"
        error[F401]: `os` imported but unused
         --> fib.py:1:8
          |
        1 | import os
          |        ^^
        help: Remove unused import: `os`

        error[F841]: Local variable `x` is assigned to but never used
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

        error[F821]: Undefined name `a`
         --> undef.py:1:4
          |
        1 | if a == 1: pass
          |    ^

        error[F821]: Undefined name `fibonaccii`
          --> fib.py:12:16
           |
        10 |         return 1
        11 |     else:
        12 |         return fibonaccii(n - 1) + fibonacci(n - 2)
           |                ^^^^^^^^^^          -
        info: Did you mean to import it from `/some/path/def.py`?
         --> fib.py:4:5
          |
        4 | def fibonacci(n):
          |     ^^^^^^^^^ `fibonacci` is defined here
        5 |     """Compute the nth number in the Fibonacci sequence."""
          |     ------------------------------------------------------- `fibonacci` is documented here
        6 |     x = 1
        7 |     if n == 0:
          |
        "###);
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
        help: Remove unused import: `os`
          |
        1 - import os
          |
        note: This is an unsafe fix and may change runtime behavior

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
          |
        6 -     x = 1
          |
        note: This is an unsafe fix and may change runtime behavior

        F821 Undefined name `a`
         --> undef.py:1:4
          |
        1 | if a == 1: pass
          |    ^

        F821 Undefined name `fibonaccii`
          --> fib.py:12:16
           |
        10 |         return 1
        11 |     else:
        12 |         return fibonaccii(n - 1) + fibonacci(n - 2)
           |                ^^^^^^^^^^          -
        info: Did you mean to import it from `/some/path/def.py`?
         --> fib.py:4:5
          |
        4 | def fibonacci(n):
          |     ^^^^^^^^^ `fibonacci` is defined here
        5 |     """Compute the nth number in the Fibonacci sequence."""
          |     ------------------------------------------------------- `fibonacci` is documented here
        6 |     x = 1
        7 |     if n == 0:
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
        1 | nested_fstrings = f'␈{f'␚{f'␛'}'}'
          |                         ^
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
        1 | ␈␚␛
          |  ^
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
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @"
        error[F401][*]: `os` imported but unused
         --> notebook.ipynb:cell 1:2:8
          |
        1 | # cell 1
        2 | import os
          |        ^^
        help: Remove unused import: `os`
         ::: cell 1:1:1
          |
        1 - import os
          |

        error[F401][*]: `math` imported but unused
         --> notebook.ipynb:cell 2:2:8
          |
        1 | # cell 2
        2 | import math
          |        ^^^^
        3 |
        4 | print('hello world')
          |
        help: Remove unused import: `math`
         ::: cell 2:1:1
          |
        1 - import math
          |

        error[F841]: Local variable `x` is assigned to but never used
         --> notebook.ipynb:cell 3:4:5
          |
        2 | def foo():
        3 |     print()
        4 |     x = 1
          |     ^
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
        help: Remove `print` statement
        ");
    }

    /// Test that we remap notebook cell line numbers in the diff as well as the main diagnostic.
    #[test]
    fn notebook_output_with_diff() {
        let (mut env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);

        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn notebook_output_with_diff_spanning_cells() {
        let (mut env, mut diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Full);
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
         --> example.py:1:16
          |
        1 | unexpected eof
          |               ^
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

        insta::assert_snapshot!(env.render(&diagnostic), @"
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
          |
        7 | fixed line 7
          | +++++
        note: This is an unsafe fix and may change runtime behavior
        ");
    }

    #[test]
    fn nearby_fix_edits_share_diff_frame() {
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
line 11
line 12
line 13
";
        env.add("example.py", contents);
        env.format(DiagnosticFormat::Full);
        env.context(0);
        env.merge_window(2);

        let replacement = |target: &str| {
            let start = contents.find(target).unwrap();
            Edit::range_replacement(
                format!("fixed {target}"),
                TextRange::at(TextSize::try_from(start).unwrap(), target.text_len()),
            )
        };

        let mut diagnostic = env.err().primary("example.py", "2", "2", "").build();
        diagnostic.help("Replace three lines");
        diagnostic.set_fix(Fix::safe_edits(
            replacement("line 2"),
            [replacement("line 7"), replacement("line 13")],
        ));

        insta::assert_snapshot!(env.render(&diagnostic), @"
        error[test-diagnostic]: main diagnostic message
         --> example.py:2:1
          |
        2 | line 2
          | ^^^^^^
        help: Replace three lines
           |
         2 ~ fixed line 2
         3 | line 3
         …
         6 | line 6
         7 ~ fixed line 7
         8 | line 8
         …
        12 | line 12
        13 ~ fixed line 13
           |
        ");
    }
}
