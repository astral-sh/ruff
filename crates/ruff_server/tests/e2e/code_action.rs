use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{CodeAction, CodeActionKind, CodeActionResolveRequest};

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
