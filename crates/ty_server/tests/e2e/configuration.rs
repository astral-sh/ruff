use anyhow::Result;
use insta::assert_json_snapshot;
use ruff_db::system::SystemPath;
use serde_json::{Map, json};
use ty_server::{ClientOptions, WorkspaceOptions};

use crate::TestServerBuilder;
use crate::pull_diagnostics::filter_result_id;

#[test]
fn configuration_file() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return a
";

    let builder = TestServerBuilder::new()?;

    let settings_path = builder.file_path("ty2.toml");

    let mut server = builder
        .with_workspace(
            workspace_root,
            Some(ClientOptions {
                workspace: WorkspaceOptions {
                    configuration_file: Some(settings_path.to_string()),
                    ..WorkspaceOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .with_file(foo, foo_content)?
        .with_file(
            settings_path,
            r#"
[rules]
unresolved-reference="warn"
        "#,
        )?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn invalid_configuration_file() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return a
";

    let builder = TestServerBuilder::new()?;

    let settings_path = builder.file_path("ty2.toml");

    let mut server = builder
        .with_workspace(
            workspace_root,
            Some(ClientOptions {
                workspace: WorkspaceOptions {
                    configuration_file: Some(settings_path.to_string()),
                    ..WorkspaceOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .with_file(foo, foo_content)?
        .with_file(
            settings_path,
            r#"
[rule]
unresolved-reference="warn"
        "#,
        )?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let show_message = server.await_notification::<lsp_types::notification::ShowMessage>();
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(show_message, @r#"
    {
      "type": 1,
      "message": "Failed to load project for workspace file://<temp_dir>/src. Please refer to the logs for more details."
    }
    "#);
    assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn configuration_overrides() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return a
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions {
                workspace: WorkspaceOptions {
                    configuration: Some(
                        Map::from_iter([(
                            "rules".to_string(),
                            json!({"unresolved-reference": "warn"}),
                        )])
                        .into(),
                    ),
                    ..WorkspaceOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn configuration_file_and_overrides() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return a
";

    let builder = TestServerBuilder::new()?;

    let settings_path = builder.file_path("ty2.toml");

    let mut server = builder
        .with_workspace(
            workspace_root,
            Some(ClientOptions {
                workspace: WorkspaceOptions {
                    configuration_file: Some(settings_path.to_string()),
                    configuration: Some(
                        Map::from_iter([(
                            "rules".to_string(),
                            json!({"unresolved-reference": "ignore"}),
                        )])
                        .into(),
                    ),
                    ..WorkspaceOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .with_file(foo, foo_content)?
        .with_file(
            settings_path,
            r#"
[rules]
unresolved-reference="warn"
        "#,
        )?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(diagnostics);

    Ok(())
}
