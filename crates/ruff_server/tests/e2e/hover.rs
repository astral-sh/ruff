use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::Position;
use lsp_types::notification::PublishDiagnostics;

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

    server.await_notification::<PublishDiagnostics>();

    let result = server.hover_request(
        "test.py",
        Position {
            line: 0,
            character: 16,
        },
    );

    assert_json_snapshot!(
        result,
        @r###"
    {
      "contents": {
        "kind": "markdown",
        "value": "# unused-noqa (RUF100)\n\nDerived from the **Ruff-specific rules** linter.\n\nFix is always available.\n\n## What it does\nChecks for `noqa` directives that are no longer applicable.\n\n## Why is this bad?\nA `noqa` directive that no longer matches any diagnostic violations is\nlikely included by mistake, and should be removed to avoid confusion.\n\n## Example\n```python\nimport foo  # noqa: F401\n\n\ndef bar():\n    foo.bar()\n```\n\nUse instead:\n```python\nimport foo\n\n\ndef bar():\n    foo.bar()\n```\n\n## Conflict with other linters\nWhen using `RUF100` with the `--fix` option, Ruff may remove trailing comments\nthat follow a `# noqa` directive on the same line, as it interprets the\nremainder of the line as a description for the suppression.\n\nTo prevent Ruff from removing suppressions for other tools (like `pylint`\nor `mypy`), separate them with a second `#` character:\n\n```python\n# Bad: Ruff --fix will remove the pylint comment\ndef visit_ImportFrom(self, node):  # noqa: N802, pylint: disable=invalid-name\n    pass\n\n\n# Good: Ruff will preserve the pylint comment\ndef visit_ImportFrom(self, node):  # noqa: N802 # pylint: disable=invalid-name\n    pass\n```\n\n## See also\n\nThis rule ignores any codes that are unknown to Ruff, as it can't determine\nif the codes are valid or used by other tools. Enable [`invalid-rule-code`][RUF102]\nto flag any unknown rule codes.\n\n## References\n- [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)\n\n[RUF102]: https://docs.astral.sh/ruff/rules/invalid-rule-code/"
      }
    }
    "###
    );

    Ok(())
}
