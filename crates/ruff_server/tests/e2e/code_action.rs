use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{
    Code, CodeAction, CodeActionKind, CodeActionResolveRequest, CodeActionResponse,
    DocumentDiagnosticReport,
};

use crate::TestServerBuilder;

fn assert_code_action_resolve_unchanged(server: &mut crate::TestServer, action: &CodeAction) {
    let request_id = server.send_request::<CodeActionResolveRequest>(action.clone());
    assert_eq!(
        &server.await_response::<CodeActionResolveRequest>(&request_id),
        action
    );
}

#[test]
fn no_code_actions_for_markdown() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document_with_language_id("test.md", "markdown", "# Hello", 1);

    let actions = server
        .code_action_request("test.md", vec![])
        .expect("Expected Some response");

    assert_json_snapshot!(actions, @"[]");

    Ok(())
}

#[test]
fn code_actions_for_python() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    server.open_text_document("test.py", "import os\n", 1);

    let actions = server
        .code_action_request("test.py", vec![])
        .expect("Expected Some response");

    assert_json_snapshot!(
        actions,
        @r#"
    [
      {
        "title": "Ruff: Fix all auto-fixable problems",
        "kind": "source.fixAll.ruff",
        "edit": {
          "changes": {
            "file://<temp_dir>/test.py": [
              {
                "range": {
                  "start": {
                    "line": 0,
                    "character": 0
                  },
                  "end": {
                    "line": 1,
                    "character": 0
                  }
                },
                "newText": ""
              }
            ]
          }
        }
      },
      {
        "title": "Ruff: Organize imports",
        "kind": "source.organizeImports.ruff",
        "edit": {
          "changes": {}
        }
      }
    ]
    "#
    );

    Ok(())
}

#[test]
fn code_actions_for_toml() -> Result<()> {
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

    let diagnostics = match server.document_diagnostic_request("ruff.toml", None) {
        DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) => {
            report.full_document_diagnostic_report.items
        }
        DocumentDiagnosticReport::RelatedUnchangedDocumentDiagnosticReport(_) => {
            panic!("Expected a full diagnostic report");
        }
    };
    let actions = server
        .code_action_request("ruff.toml", diagnostics)
        .expect("Expected code actions");

    assert_json_snapshot!(actions, @r#"
    [
      {
        "title": "Ruff (rule-codes-in-selectors): Replace rule code with name",
        "kind": "quickfix",
        "diagnostics": [
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
            "message": "Rule code used instead of name in `lint.extend-select`\n\nhelp: Replace rule code with name",
            "tags": []
          }
        ],
        "edit": {
          "changes": {
            "file://<temp_dir>/ruff.toml": [
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
                "newText": "unused-import"
              }
            ]
          }
        },
        "data": "file://<temp_dir>/ruff.toml"
      },
      {
        "title": "Ruff: Fix all auto-fixable problems",
        "kind": "source.fixAll.ruff",
        "edit": {
          "changes": {
            "file://<temp_dir>/ruff.toml": [
              {
                "range": {
                  "start": {
                    "line": 4,
                    "character": 0
                  },
                  "end": {
                    "line": 5,
                    "character": 0
                  }
                },
                "newText": "extend-select = [\"unused-import\"]\n"
              }
            ]
          }
        }
      }
    ]
    "#);

    Ok(())
}

#[test]
fn human_readable_rule_names() -> Result<()> {
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

    server.open_text_document("test.py", "import os\n", 1);

    let diagnostics = match server.document_diagnostic_request("test.py", None) {
        DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) => {
            report.full_document_diagnostic_report.items
        }
        DocumentDiagnosticReport::RelatedUnchangedDocumentDiagnosticReport(_) => {
            panic!("Expected a full diagnostic report");
        }
    };
    assert_eq!(
        diagnostics[0].code,
        Some(Code::String("unused-import".to_string()))
    );

    let actions = server
        .code_action_request("test.py", diagnostics)
        .expect("Expected code actions");
    let titles: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            CodeActionResponse::CodeAction(action) => Some(action.title.as_str()),
            CodeActionResponse::Command(_) => None,
        })
        .collect();

    assert!(titles.contains(&"Ruff (unused-import): Remove unused import: `os`"));
    assert!(titles.contains(&"Ruff (unused-import): Disable for this line"));

    Ok(())
}

#[test]
fn code_action_without_valid_url_returns_unchanged_action() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let action = CodeAction {
        title: "Some other code action".to_string(),
        kind: Some(CodeActionKind::QuickFix),
        ..Default::default()
    };

    assert_code_action_resolve_unchanged(&mut server, &action);

    Ok(())
}

#[test]
fn invalid_code_action_resolve_data_returns_unchanged_action() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let action = CodeAction {
        title: "Ruff: Fix all auto-fixable problems".to_string(),
        kind: Some(CodeActionKind::from("source.fixAll.ruff")),
        data: Some(serde_json::json!("not-a-uri")),
        ..Default::default()
    };

    assert_code_action_resolve_unchanged(&mut server, &action);

    Ok(())
}
