use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;

use anyhow::{Context, Result, bail};
use insta::assert_json_snapshot;
use lsp_types::{
    Code, CodeActionKind, CodeActionResponse, Diagnostic, DocumentDiagnosticReport, Message,
};
use test_case::test_case;

use crate::{TestServer, TestServerBuilder};

fn diagnostic_items(server: &mut TestServer, document: &str) -> Result<Vec<Diagnostic>> {
    let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) =
        server.document_diagnostic_request(document, None)
    else {
        bail!("Expected a full diagnostic report");
    };
    Ok(report.full_document_diagnostic_report.items)
}

#[test_case("app/TEST.py"; "original_uri")]
#[test_case("app/test.py"; "lowercase_uri")]
fn module_name_after_case_only_rename(document: &str) -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("ruff.toml", r#"lint.select = ["N999"]"#)?
        .with_file("app/__init__.py", "")?
        .with_file("app/TEST.py", "# ruff: noqa: N999\n")?
        .build();
    let uppercase = server.file_path("app/TEST.py");
    let lowercase = server.file_path("app/test.py");
    if !lowercase.try_exists()? {
        // The two spellings must refer to the same file.
        return Ok(());
    }

    // The open buffer has no suppression.
    server.open_text_document(document, "", 1);
    let items = diagnostic_items(&mut server, document)?;
    let [diagnostic] = items.as_slice() else {
        bail!("Expected one N999 diagnostic, got {items:?}");
    };
    assert_eq!(diagnostic.code, Some(Code::String("N999".into())));
    assert_eq!(
        diagnostic.message,
        Message::String("Invalid module name: 'TEST'".into())
    );

    fs::rename(uppercase, lowercase)?;
    let items = diagnostic_items(&mut server, document)?;
    assert!(
        items.is_empty(),
        "Unexpected diagnostics after rename: {items:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn module_name_uses_editor_spelling_for_symlink() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("ruff.toml", r#"lint.select = ["N999"]"#)?
        .with_file("app/__init__.py", "")?
        .with_file("app/TEST.py", "")?
        .build();
    let alias = server.file_path("app/alias.py");
    symlink("TEST.py", &alias)?;

    server.open_text_document("app/alias.py", "", 1);
    assert!(diagnostic_items(&mut server, "app/alias.py")?.is_empty());

    let uppercase = server.file_path("app/ALIAS.py");
    if uppercase.try_exists()? {
        fs::rename(alias, uppercase)?;
        assert!(diagnostic_items(&mut server, "app/alias.py")?.is_empty());
    }
    Ok(())
}

#[test]
fn per_file_ignore_after_case_only_rename() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("ruff.toml", r#"lint.per-file-ignores."test.py" = ["F401"]"#)?
        .with_file("TEST.py", "")?
        .build();
    let lowercase = server.file_path("test.py");
    if !lowercase.try_exists()? {
        return Ok(());
    }

    server.open_text_document("TEST.py", "import os\n", 1);
    let items = diagnostic_items(&mut server, "TEST.py")?;
    let [diagnostic] = items.as_slice() else {
        bail!("Expected one F401 diagnostic, got {items:?}");
    };
    assert_eq!(diagnostic.code, Some(Code::String("F401".into())));

    fs::rename(server.file_path("TEST.py"), lowercase)?;
    assert!(diagnostic_items(&mut server, "TEST.py")?.is_empty());

    let actions = server
        .code_action_request("TEST.py", vec![])
        .context("Expected code actions")?;
    let changes = actions
        .into_iter()
        .find_map(|action| match action {
            CodeActionResponse::CodeAction(action)
                if action.kind == Some(CodeActionKind::from("source.fixAll.ruff")) =>
            {
                action.edit?.changes
            }
            _ => None,
        })
        .context("Expected a fix-all workspace edit")?;
    assert!(changes.is_empty(), "Unexpected fix-all edits: {changes:?}");
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
