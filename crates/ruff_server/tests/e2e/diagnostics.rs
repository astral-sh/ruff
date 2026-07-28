use anyhow::Result;
use insta::assert_json_snapshot;

use crate::TestServerBuilder;

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
