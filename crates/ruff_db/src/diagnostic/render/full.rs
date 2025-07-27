#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat, Severity,
        render::tests::{TestEnvironment, create_diagnostics, create_syntax_error_diagnostics},
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
        error[invalid-syntax]: SyntaxError: Expected one or more symbol names after import
         --> syntax_errors.py:1:15
          |
        1 | from os import
          |               ^
        2 |
        3 | if call(foo
          |

        error[invalid-syntax]: SyntaxError: Expected ')', found newline
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

        insta::assert_snapshot!(env.render(&diagnostic), @r#""#);

        Ok(())
    }
}
