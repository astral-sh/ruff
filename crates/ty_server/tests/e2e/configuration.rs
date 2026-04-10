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
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn unsupported_editor_python_version() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let main = SystemPath::new("src/main.py");
    let python_home = "base/bin";
    let base_python = if cfg!(target_os = "windows") {
        "base/bin/python.exe"
    } else {
        "base/bin/python"
    };
    let python = if cfg!(target_os = "windows") {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    };
    let site_packages_foo = if cfg!(target_os = "windows") {
        "venv/Lib/site-packages/foo.py"
    } else {
        "venv/lib/python3.16/site-packages/foo.py"
    };
    // The import proves we still use the editor-selected environment for module resolution even
    // when we ignore its unsupported reported Python version.
    let foo_content = "\
import foo
import sys
from typing_extensions import reveal_type

reveal_type(sys.version_info[:2])
";

    let builder = TestServerBuilder::new()?;
    let python_home = builder.file_path(python_home);
    let sys_prefix = builder.file_path("venv");
    let python_uri = builder.file_uri(python);

    let workspace_options: ClientOptions = serde_json::from_value(json!({
        "pythonExtension": {
            "activeEnvironment": {
                "executable": {
                    "uri": python_uri,
                    "sysPrefix": sys_prefix,
                },
                "version": {
                    "major": 3,
                    "minor": 16,
                    "patch": 0,
                    "sysVersion": "3.16.0",
                }
            }
        }
    }))?;

    let mut server = builder
        .with_workspace(workspace_root, Some(workspace_options))?
        .with_file(main, foo_content)?
        .with_file(base_python, "")?
        .with_file(python, "")?
        .with_file(
            "venv/pyvenv.cfg",
            format!("version_info = 3.16.0\nhome = {python_home}\n"),
        )?
        .with_file(site_packages_foo, "")?
        .build()
        .wait_until_workspaces_are_initialized();

    // The unsupported version inferred from the selected environment now surfaces as a
    // settings diagnostic on the environment's `pyvenv.cfg`.
    server.collect_publish_diagnostic_notifications(1);

    server.open_text_document(main, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(main, None);

    assert_json_snapshot!(diagnostics, @r#"
    {
      "kind": "full",
      "resultId": "[RESULT_ID]",
      "items": [
        {
          "range": {
            "start": {
              "line": 4,
              "character": 12
            },
            "end": {
              "line": 4,
              "character": 32
            }
          },
          "severity": 3,
          "code": "revealed-type",
          "source": "ty",
          "message": "Revealed type: `tuple[Literal[3], Literal[14]]`"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn unsupported_inferred_python_version_setting_diagnostic() -> Result<()> {
    let workspace_root = SystemPath::new("project");
    let main = SystemPath::new("project/main.py");
    let python_home = "base/bin";
    let base_python = if cfg!(target_os = "windows") {
        "base/bin/python.exe"
    } else {
        "base/bin/python"
    };
    let python = if cfg!(target_os = "windows") {
        "project/.venv/Scripts/python.exe"
    } else {
        "project/.venv/bin/python"
    };
    let site_packages = if cfg!(target_os = "windows") {
        "project/.venv/Lib/site-packages/foo.py"
    } else {
        "project/.venv/lib/python3.16/site-packages/foo.py"
    };

    let builder = TestServerBuilder::new()?;
    let python_home = builder.file_path(python_home);

    let mut server = builder
        .with_workspace(workspace_root, None)?
        .with_file(main, "x = 1\n")?
        .with_file(base_python, "")?
        .with_file(python, "")?
        .with_file(
            "project/.venv/pyvenv.cfg",
            format!("home = {python_home}\n"),
        )?
        .with_file(site_packages, "")?
        .build()
        .wait_until_workspaces_are_initialized();

    let diagnostics = server.collect_publish_diagnostic_notifications(1);

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
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo, None);

    assert_json_snapshot!(diagnostics);

    Ok(())
}
