use crate::TestServerBuilder;
use insta::assert_json_snapshot;

#[test]
fn simple_rename() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("old_module.py", "")?
        .with_file("consumer.py", "")?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("old_module.py", "x = 1\n", 1);
    server.open_text_document(
        "consumer.py",
        "import old_module\n\nprint(old_module.x)\n",
        1,
    );

    let old_uri = server.file_uri("old_module.py").to_string();
    let new_uri = server.file_uri("new_module.py").to_string();

    let edits = server.will_rename_files(vec![lsp_types::FileRename { old_uri, new_uri }]);

    assert_json_snapshot!(edits);

    Ok(())
}
