use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{Position, Range};

use crate::TestServerBuilder;

#[test]
fn no_hover_for_markdown() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document_with_language_id("test.md", "markdown", "# noqa: RUF100\n", 1);

    let result = server.hover_request(
        "test.md",
        Position {
            line: 0,
            character: 9,
        },
    );

    assert!(result.is_none(), "Expected no hover for markdown file");

    Ok(())
}

#[test]
fn hover_for_python_noqa() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document("test.py", "x = 1  # noqa: RUF100\n", 1);

    let result = server.hover_request(
        "test.py",
        Position {
            line: 0,
            character: 16,
        },
    );

    assert_json_snapshot!(
        result,
        @r##"
    {
      "contents": {
        "kind": "markdown",
        "value": "# unused-noqa (RUF100)\n\nDerived from the **Ruff-specific rules** linter.\n\nFix is always available.\n\n## What it does\nChecks for `noqa` directives that are no longer applicable.\n\n## Why is this bad?\nA `noqa` directive that no longer matches any diagnostic violations is\nlikely included by mistake, and should be removed to avoid confusion.\n\n## Example\n```python\nimport foo  # noqa: F401\n\n\ndef bar():\n    foo.bar()\n```\n\nUse instead:\n```python\nimport foo\n\n\ndef bar():\n    foo.bar()\n```\n\n## Conflict with other linters\nWhen using `RUF100` with the `--fix` option, Ruff may remove trailing comments\nthat follow a `# noqa` directive on the same line, as it interprets the\nremainder of the line as a description for the suppression.\n\nTo prevent Ruff from removing suppressions for other tools (like `pylint`\nor `mypy`), separate them with a second `#` character:\n\n```python\n# Bad: Ruff --fix will remove the pylint comment\ndef visit_ImportFrom(self, node):  # noqa: N802, pylint: disable=invalid-name\n    pass\n\n\n# Good: Ruff will preserve the pylint comment\ndef visit_ImportFrom(self, node):  # noqa: N802 # pylint: disable=invalid-name\n    pass\n```\n\n## Fix safety\n\nThe rule's fix is marked as unsafe when a full suppression comment would be removed and there\nare other nested comments on the same line. Removing such a comment can change the behavior of\nother suppression comments before or after the removed comment.\n\n## See also\n\nThis rule ignores any codes that are unknown to Ruff, as it can't determine\nif the codes are valid or used by other tools. Enable [`invalid-rule-code`][RUF102]\nto flag any unknown rule codes.\n\n## References\n- [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)\n\n[RUF102]: https://docs.astral.sh/ruff/rules/invalid-rule-code/"
      },
      "range": {
        "start": {
          "line": 0,
          "character": 15
        },
        "end": {
          "line": 0,
          "character": 21
        }
      }
    }
    "##
    );

    Ok(())
}

#[test]
fn hover_for_file_level_noqa() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document("test.py", "# ruff: noqa: F401\n", 1);

    let result = server
        .hover_request(
            "test.py",
            Position {
                line: 0,
                character: 15,
            },
        )
        .expect("Expected hover information for a file-level `noqa` directive");

    let lsp_types::Contents::MarkupContent(markup) = result.contents else {
        panic!("Expected Markdown hover contents");
    };
    assert!(markup.value.starts_with("# unused-import (F401)"));

    Ok(())
}

#[test]
fn hover_for_ruff_suppression_comments() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document(
        "test.py",
        "# ruff: disable[F401]\n# ruff: enable[F401]\n# ruff: file-ignore[F401]\n# ruff: ignore[F401]\n# ruff: disable[F401] # ruff: ignore[F401]\n# ruff: ignore[F401,]\n",
        1,
    );

    for position in [
        Position {
            line: 0,
            character: 17,
        },
        Position {
            line: 1,
            character: 16,
        },
        Position {
            line: 2,
            character: 21,
        },
        Position {
            line: 3,
            character: 16,
        },
        Position {
            line: 4,
            character: 38,
        },
        Position {
            line: 5,
            character: 16,
        },
    ] {
        let result = server
            .hover_request("test.py", position)
            .expect("Expected hover information for a Ruff suppression comment");

        let lsp_types::Contents::MarkupContent(markup) = result.contents else {
            panic!("Expected Markdown hover contents");
        };
        assert!(markup.value.starts_with("# unused-import (F401)"));
    }

    Ok(())
}

#[test]
fn hover_for_human_readable_rule_name() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file(
            "pyproject.toml",
            r#"
[tool.ruff]
preview = true
"#,
        )?
        .build();

    server.open_text_document("test.py", "éééé = 1  # ruff: ignore[unused-import]\n", 1);

    let result = server
        .hover_request(
            "test.py",
            Position {
                line: 0,
                character: 26,
            },
        )
        .expect("Expected hover information for a human-readable rule name");

    let lsp_types::Contents::MarkupContent(markup) = result.contents else {
        panic!("Expected Markdown hover contents");
    };
    assert!(markup.value.starts_with("# unused-import (F401)"));
    assert_eq!(
        result.range,
        Some(Range {
            start: Position {
                line: 0,
                character: 25,
            },
            end: Position {
                line: 0,
                character: 38,
            },
        })
    );

    Ok(())
}

#[test]
fn no_hover_for_suppression_text_in_string() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document(
        "test.py",
        "\"\"\"\nnot a comment # ruff:ignore[unused-import]\n\"\"\"\n",
        1,
    );

    let result = server.hover_request(
        "test.py",
        Position {
            line: 1,
            character: 31,
        },
    );

    assert!(result.is_none());

    Ok(())
}

#[test]
fn unavailable_document_returns_empty_response() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let result = server.hover_request("not-open.py", Position::default());

    assert!(result.is_none());

    Ok(())
}
