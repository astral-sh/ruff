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
        .build();

    server.open_text_document("systemtests/tests/common/fakes/wus.py", "import os\n", 1);

    let diagnostics =
        server.document_diagnostic_request("systemtests/tests/common/fakes/wus.py", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "kind": "full",
      "items": []
    }
    "#
    );

    Ok(())
}
