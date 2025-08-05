#[cfg(test)]
mod tests {
    use ruff_diagnostics::Applicability;
    use ruff_text_size::TextRange;

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
    /// error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1A" instead
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
                r#"Invalid unescaped character SUB, use "\x1A" instead"#,
            )
            .primary("example.py", "1:24", "1:24", "")
            .build();

        insta::assert_snapshot!(env.render(&diagnostic), @r#"
        error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1A" instead
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
                r#"Invalid unescaped character SUB, use "\x1A" instead"#,
            )
            .primary("example.py", "1:1", "1:1", "")
            .build();

        insta::assert_snapshot!(env.render(&diagnostic), @r#"
        error[invalid-character-sub]: Invalid unescaped character SUB, use "\x1A" instead
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
}
