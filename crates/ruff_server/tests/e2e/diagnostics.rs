use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{
    DocumentDiagnosticReport, TextDocumentContentChangeEvent,
    TextDocumentContentChangeWholeDocument,
};

use crate::TestServerBuilder;

#[test]
fn script_requires_python_in_open_document() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file(
            "ruff.toml",
            "per-file-target-version = { 'script.py' = 'py39' }\nlint.select = ['UP045']",
        )?
        .build();
    let code = "from typing import Optional\nx: Optional[int]\n";
    server.open_text_document(
        "script.py",
        format!("# /// script\n# requires-python = '>=3.10'\n# ///\n{code}"),
        1,
    );
    let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) =
        server.document_diagnostic_request("script.py", None)
    else {
        anyhow::bail!("expected a full report");
    };
    assert_eq!(report.full_document_diagnostic_report.items.len(), 1);
    server.change_text_document(
        "script.py",
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument { text: code.into() },
            ),
        ],
        2,
    );
    let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) =
        server.document_diagnostic_request("script.py", None)
    else {
        anyhow::bail!("expected a full report after editing the script");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());
    Ok(())
}

#[test]
fn uses_human_readable_names_in_preview() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", "tool.ruff.preview = true")?
        .build();

    server.open_text_document("test.py", "import os\n", 1);

    let diagnostics = server.document_diagnostic_request("test.py", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "items": [
        {
          "range": {
            "start": {
              "line": 0,
              "character": 7
            },
            "end": {
              "line": 0,
              "character": 9
            }
          },
          "severity": 2,
          "code": "unused-import",
          "codeDescription": {
            "href": "https://docs.astral.sh/ruff/rules/unused-import"
          },
          "source": "Ruff",
          "message": "`os` imported but unused\n\nhelp: Remove unused import: `os`",
          "tags": [
            1
          ],
          "data": {
            "code": "unused-import",
            "edits": [
              {
                "newText": "",
                "range": {
                  "end": {
                    "character": 0,
                    "line": 1
                  },
                  "start": {
                    "character": 0,
                    "line": 0
                  }
                }
              }
            ],
            "is_preferred": true,
            "noqa_edit": {
              "newText": "  # ruff: ignore[unused-import]\n",
              "range": {
                "end": {
                  "character": 0,
                  "line": 1
                },
                "start": {
                  "character": 9,
                  "line": 0
                }
              }
            },
            "title": "Remove unused import: `os`"
          }
        }
      ],
      "kind": "full"
    }
    "#
    );

    Ok(())
}

#[test]
fn toml_diagnostics() -> Result<()> {
    let source = r#"
[lint]
preview = true
select = ["rule-codes-in-selectors"]
extend-select = ["F401"]
"#;
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("ruff.toml", source)?
        .build();

    server.open_text_document_with_language_id("ruff.toml", "toml", source, 1);

    let diagnostics = server.document_diagnostic_request("ruff.toml", None);

    assert_json_snapshot!(diagnostics, @r#"
    {
      "items": [
        {
          "range": {
            "start": {
              "line": 4,
              "character": 18
            },
            "end": {
              "line": 4,
              "character": 22
            }
          },
          "severity": 2,
          "code": "rule-codes-in-selectors",
          "codeDescription": {
            "href": "https://docs.astral.sh/ruff/rules/rule-codes-in-selectors"
          },
          "source": "Ruff",
          "message": "Rule code used instead of name in `lint.extend-select`\n\nhelp: Replace rule code with `unused-import`",
          "tags": [],
          "data": {
            "code": "rule-codes-in-selectors",
            "edits": [
              {
                "newText": "unused-import",
                "range": {
                  "end": {
                    "character": 22,
                    "line": 4
                  },
                  "start": {
                    "character": 18,
                    "line": 4
                  }
                }
              }
            ],
            "is_preferred": true,
            "noqa_edit": null,
            "title": "Replace rule code with `unused-import`"
          }
        }
      ],
      "kind": "full"
    }
    "#);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_diagnostic() -> Result<()> {
    let source = "[project]\nname = 1\n";
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document_with_language_id("pyproject.toml", "toml", source, 1);

    let diagnostics = server.document_diagnostic_request("pyproject.toml", None);

    assert_json_snapshot!(diagnostics, @r#"
    {
      "items": [
        {
          "range": {
            "start": {
              "line": 1,
              "character": 7
            },
            "end": {
              "line": 1,
              "character": 8
            }
          },
          "severity": 2,
          "code": "RUF200",
          "codeDescription": {
            "href": "https://docs.astral.sh/ruff/rules/invalid-pyproject-toml"
          },
          "source": "Ruff",
          "message": "Failed to parse pyproject.toml: invalid type: integer `1`, expected a string",
          "tags": []
        }
      ],
      "kind": "full"
    }
    "#);

    Ok(())
}
