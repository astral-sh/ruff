use anyhow::Result;
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn folding_range_basic_functionality() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"class MyClass:
    def __init__(self):
        self.value = 1

    def method(self):
        return self.value
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let ranges = server.folding_range_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(ranges);

    Ok(())
}
