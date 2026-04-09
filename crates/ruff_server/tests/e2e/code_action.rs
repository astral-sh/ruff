use anyhow::Result;
use insta::assert_json_snapshot;

use crate::TestServerBuilder;

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
