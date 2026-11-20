use anyhow::Result;
use indoc::indoc;
use insta::assert_json_snapshot;
use lsp_types::{
    DocumentDiagnosticReport, TextDocumentContentChangeEvent,
    TextDocumentContentChangeWholeDocument,
};

use crate::{TestServer, TestServerBuilder};

#[test]
fn invalid_script_configuration_blocks_edits() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file(
            "ruff.toml",
            indoc! {r#"
                preview = true
                lint.ignore = ['F401']
            "#},
        )?
        .build();
    server.open_text_document(
        "script.py",
        indoc! {r#"
            #!/usr/bin/env python

            # /// script
            # tool.ruff.line-length = 'wide'
            # ///
            import os
            x= 1
        "#},
        1,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].code,
        Some(lsp_types::Code::String(
            "invalid-script-metadata".to_owned()
        ))
    );
    assert_eq!(
        items[0].range,
        lsp_types::Range::new(
            lsp_types::Position::new(3, 26),
            lsp_types::Position::new(3, 32),
        )
    );
    assert!(
        server
            .code_action_request("script.py", items)
            .unwrap_or_default()
            .is_empty()
    );
    assert!(
        server
            .format_request("script.py")
            .is_none_or(|edits| edits.is_empty())
    );
    Ok(())
}

#[test]
fn inline_script_preserves_project_exclusions() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file(
            "ruff.toml",
            indoc! {r#"
                preview = true
                exclude = ['script.py']
            "#},
        )?
        .build();
    server.open_text_document(
        "script.py",
        indoc! {r#"
            # /// script
            # [tool.ruff]
            # ///
            import os
        "#},
        1,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert!(items.is_empty());
    Ok(())
}

#[test]
fn inline_script_configuration_in_open_document() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("ruff.toml", "lint.ignore = ['F401']")?
        .with_server_preview(true)
        .build();
    server.open_text_document(
        "script.py",
        indoc! {r#"
            # /// script
            # [tool.ruff]
            # ///
            import os
        "#},
        1,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].code,
        Some(lsp_types::Code::String("unused-import".to_owned()))
    );

    server.change_text_document(
        "script.py",
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: indoc! {r#"
                        import os
                    "#}
                    .into(),
                },
            ),
        ],
        2,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert!(items.is_empty());
    Ok(())
}

#[test]
fn script_requires_python_in_open_document() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file(
            "ruff.toml",
            indoc! {r#"
                preview = true
                per-file-target-version = { 'script.py' = 'py39' }
                lint.select = ['UP045']
            "#},
        )?
        .build();
    let code = indoc! {r#"
        from typing import Optional
        x: Optional[int]
    "#};
    server.open_text_document(
        "script.py",
        indoc! {r#"
            # /// script
            # requires-python = '>=3.10'
            # ///
            from typing import Optional
            x: Optional[int]
        "#},
        1,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert_eq!(items.len(), 1);
    server.change_text_document(
        "script.py",
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument { text: code.into() },
            ),
        ],
        2,
    );
    let items = document_diagnostics(&mut server, "script.py")?;
    assert!(items.is_empty());
    Ok(())
}

fn document_diagnostics(server: &mut TestServer, path: &str) -> Result<Vec<lsp_types::Diagnostic>> {
    let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) =
        server.document_diagnostic_request(path, None)
    else {
        anyhow::bail!("expected a full diagnostic report for {path}");
    };
    Ok(report.full_document_diagnostic_report.items)
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
