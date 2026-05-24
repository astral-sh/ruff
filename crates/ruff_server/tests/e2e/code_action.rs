use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_server::ErrorCode;
use lsp_types::{CodeAction, CodeActionKind, request::CodeActionResolveRequest};

use crate::{AwaitResponseError, TestServerBuilder};

fn assert_code_action_resolve_internal_error(
    server: &mut crate::TestServer,
    action: CodeAction,
    expected_message: &str,
) {
    let request_id = server.send_request::<CodeActionResolveRequest>(action);

    let error = match server.try_await_response::<CodeActionResolveRequest>(&request_id, None) {
        Err(AwaitResponseError::RequestFailed(error)) => error,
        result => panic!("Expected an InternalError response, got {result:?}"),
    };

    assert_eq!(error.code, ErrorCode::InternalError as i32);
    assert_eq!(error.message, expected_message);
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
fn missing_code_action_resolve_data_returns_internal_error() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let action = CodeAction {
        title: "Ruff: Fix all auto-fixable problems".to_string(),
        kind: Some(CodeActionKind::from("source.fixAll.ruff")),
        ..Default::default()
    };

    assert_code_action_resolve_internal_error(
        &mut server,
        action,
        "Code action is missing Ruff's document URI payload",
    );

    Ok(())
}

#[test]
fn invalid_code_action_resolve_data_returns_internal_error() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let action = CodeAction {
        title: "Ruff: Fix all auto-fixable problems".to_string(),
        kind: Some(CodeActionKind::from("source.fixAll.ruff")),
        data: Some(serde_json::json!("not-a-url")),
        ..Default::default()
    };

    let request_id = server.send_request::<CodeActionResolveRequest>(action);

    let error = match server.try_await_response::<CodeActionResolveRequest>(&request_id, None) {
        Err(AwaitResponseError::RequestFailed(error)) => error,
        result => panic!("Expected an InternalError response, got {result:?}"),
    };

    assert_eq!(error.code, ErrorCode::InternalError as i32);
    assert_json_snapshot!(error, @r#"
    {
      "code": -32603,
      "message": "Code action has an invalid Ruff document URI payload: relative URL without a base: \"not-a-url\""
    }
    "#);

    Ok(())
}
