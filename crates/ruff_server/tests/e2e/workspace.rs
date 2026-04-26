use anyhow::Result;
use insta::assert_json_snapshot;

use crate::TestServerBuilder;

#[test]
fn selects_the_correct_workspace_settings_for_multi_root_workspaces() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_workspace("external/Y")?
        .with_file(
            "systemtests/pyproject.toml",
            r#"
[tool.ruff.lint]
ignore = ["F401"]
"#,
        )?
        .with_file("systemtests/tests/common/fakes/wus.py", "import os\n")?
        .with_file("external/Y/wus.py", "import os\n")?
        .build();

    server.open_text_document("systemtests/tests/common/fakes/wus.py", "import os\n", 1);
    server.open_text_document("external/Y/wus.py", "import os\n", 1);

    let diagnostics =
        server.document_diagnostic_request("systemtests/tests/common/fakes/wus.py", None);
    let external_diagnostics = server.document_diagnostic_request("external/Y/wus.py", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "items": [],
      "kind": "full"
    }
    "#
    );

    assert_json_snapshot!(
        external_diagnostics,
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
          "code": "F401",
          "codeDescription": {
            "href": "https://docs.astral.sh/ruff/rules/unused-import"
          },
          "source": "Ruff",
          "message": "`os` imported but unused",
          "tags": [
            1
          ],
          "data": {
            "code": "F401",
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
            "noqa_edit": {
              "newText": "  # noqa: F401\n",
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
