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

#[test]
fn batch_rename() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("old_a.py", "")?
        .with_file("old_b.py", "")?
        .with_file("consumer.py", "")?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("old_a.py", "a = 1\n", 1);
    server.open_text_document("old_b.py", "b = 1\n", 1);
    server.open_text_document(
        "consumer.py",
        "import old_a\nimport old_b\n\nprint(old_a.a, old_b.b)\n",
        1,
    );

    // A single request renaming two modules should produce edits for both in one
    // pass over the project's files.
    let renames = vec![
        lsp_types::FileRename {
            old_uri: server.file_uri("old_a.py").to_string(),
            new_uri: server.file_uri("new_a.py").to_string(),
        },
        lsp_types::FileRename {
            old_uri: server.file_uri("old_b.py").to_string(),
            new_uri: server.file_uri("new_b.py").to_string(),
        },
    ];

    let edits = server.will_rename_files(renames);

    assert_json_snapshot!(edits);

    Ok(())
}

#[test]
fn directory_rename() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("old_pkg/__init__.py", "")?
        .with_file("consumer.py", "")?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("old_pkg/__init__.py", "x = 1\n", 1);
    server.open_text_document("consumer.py", "from old_pkg import x\n\nprint(x)\n", 1);

    let old_uri = server.file_uri("old_pkg").to_string();
    let new_uri = server.file_uri("new_pkg").to_string();

    let edits = server.will_rename_files(vec![lsp_types::FileRename { old_uri, new_uri }]);

    assert_json_snapshot!(edits);

    Ok(())
}
