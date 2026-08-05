use anyhow::Result;
use lsp_types::Position;
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn script_search_paths_resolve_imported_symbols() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let dependency = SystemPath::new("src/dependencies/dependency.py");

    let script = SystemPath::new("src/script.py");
    let script_content = r#"# /// script
# [tool.ty.environment]
# extra-paths = ["./dependencies"]
# ///

from dependency import script_only
"#;

    let ordinary = SystemPath::new("src/ordinary.py");
    let ordinary_content = "from dependency import script_only\n";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(dependency, "def script_only() -> None: ...\n")?
        .with_file(script, script_content)?
        .with_file(ordinary, ordinary_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(script, script_content, 1);
    server.open_text_document(ordinary, ordinary_content, 1);

    let script_definition = server.goto_definition_request(script, Position::new(5, 24));
    insta::assert_json_snapshot!(script_definition, @r#"
    [
      {
        "uri": "file://<temp_dir>/src/dependencies/dependency.py",
        "range": {
          "start": {
            "line": 0,
            "character": 4
          },
          "end": {
            "line": 0,
            "character": 15
          }
        }
      }
    ]
    "#);

    let ordinary_definition = server.goto_definition_request(ordinary, Position::new(0, 24));
    insta::assert_json_snapshot!(ordinary_definition, @"null");

    Ok(())
}
