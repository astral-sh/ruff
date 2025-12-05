use anyhow::Result;
use lsp_types::{Position, notification::ShowMessage, request::RegisterCapability};
use ruff_db::system::SystemPath;
use serde_json::Value;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::TestServerBuilder;

#[test]
fn empty_workspace_folders() -> Result<()> {
    let server = TestServerBuilder::new()?
        .build()
        .wait_until_workspaces_are_initialized();

    let initialization_result = server.initialization_result().unwrap();

    insta::assert_json_snapshot!("initialization", initialization_result);

    Ok(())
}

#[test]
fn single_workspace_folder() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    let initialization_result = server.initialization_result().unwrap();

    insta::assert_json_snapshot!("initialization_with_workspace", initialization_result);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if workspace diagnostics
/// are enabled via initialization options and dynamic registration is enabled, even if the
/// workspace configuration is not supported by the client.
#[test]
fn workspace_diagnostic_registration_without_configuration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_workspace(workspace_root, None)?
        .enable_workspace_configuration(false)
        .enable_diagnostic_dynamic_registration(true)
        .build();

    // No need to wait for workspaces to initialize as the client does not support workspace
    // configuration.

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": true,
        "workspaceDiagnostics": true
      }
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if open files diagnostics
/// are enabled via initialization options and dynamic registration is enabled, even if the
/// workspace configuration is not supported by the client.
#[test]
fn open_files_diagnostic_registration_without_configuration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::OpenFilesOnly),
        )
        .with_workspace(workspace_root, None)?
        .enable_workspace_configuration(false)
        .enable_diagnostic_dynamic_registration(true)
        .build();

    // No need to wait for workspaces to initialize as the client does not support workspace
    // configuration.

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": false,
        "workspaceDiagnostics": false
      }
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if workspace diagnostics
/// are enabled via initialization options and dynamic registration is enabled.
#[test]
fn workspace_diagnostic_registration_via_initialization() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_workspace(workspace_root, None)?
        .enable_diagnostic_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": true,
        "workspaceDiagnostics": true
      }
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if open files diagnostics
/// are enabled via initialization options and dynamic registration is enabled.
#[test]
fn open_files_diagnostic_registration_via_initialization() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::OpenFilesOnly),
        )
        .with_workspace(workspace_root, None)?
        .enable_diagnostic_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": false,
        "workspaceDiagnostics": false
      }
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if workspace diagnostics
/// are enabled and dynamic registration is enabled.
#[test]
fn workspace_diagnostic_registration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
        )?
        .enable_diagnostic_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": true,
        "workspaceDiagnostics": true
      }
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if workspace diagnostics are
/// disabled and dynamic registration is enabled.
#[test]
fn open_files_diagnostic_registration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::OpenFilesOnly)),
        )?
        .enable_diagnostic_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/diagnostic",
      "method": "textDocument/diagnostic",
      "registerOptions": {
        "documentSelector": null,
        "identifier": "ty",
        "interFileDependencies": true,
        "workDoneProgress": false,
        "workspaceDiagnostics": false
      }
    }
    "#);

    Ok(())
}

/// Tests that the server can disable language services for a workspace via initialization options.
#[test]
fn disable_language_services_set_on_initialization() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_disable_language_services(true))
        .with_workspace(workspace_root, None)?
        .enable_pull_diagnostics(true)
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let hover = server.hover_request(foo, Position::new(0, 5));

    assert!(
        hover.is_none(),
        "Expected no hover information, got: {hover:?}"
    );

    Ok(())
}

/// Tests that the server can disable language services for a workspace via workspace configuration
/// request.
#[test]
fn disable_language_services_set_on_workspace() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_disable_language_services(true)),
        )?
        .enable_pull_diagnostics(true)
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let hover = server.hover_request(foo, Position::new(0, 5));

    assert!(
        hover.is_none(),
        "Expected no hover information, got: {hover:?}"
    );

    Ok(())
}

/// Tests that the server can disable language services for one workspace while keeping them
/// enabled for another.
#[test]
#[ignore = "Requires multiple workspace support in the server and test server"]
fn disable_language_services_for_one_workspace() -> Result<()> {
    let workspace_a = SystemPath::new("src/a");
    let workspace_b = SystemPath::new("src/b");
    let foo = SystemPath::new("src/a/foo.py");
    let bar = SystemPath::new("src/b/bar.py");
    let foo_content = "\
def foo() -> str:
    return 42
";
    let bar_content = "\
def bar() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_a,
            Some(ClientOptions::default().with_disable_language_services(true)),
        )?
        .with_workspace(workspace_b, None)?
        .enable_pull_diagnostics(true)
        .with_file(foo, foo_content)?
        .with_file(bar, bar_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let hover_foo = server.hover_request(foo, Position::new(0, 5));
    assert!(
        hover_foo.is_none(),
        "Expected no hover information for workspace A, got: {hover_foo:?}"
    );

    server.open_text_document(bar, bar_content, 1);
    let hover_bar = server.hover_request(bar, Position::new(0, 5));
    assert!(
        hover_bar.is_some(),
        "Expected hover information for workspace B, got: {hover_bar:?}"
    );

    Ok(())
}

/// Tests that the server sends a warning notification if user provided unknown options during
/// initialization.
#[test]
fn unknown_initialization_options() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_initialization_options(
            ClientOptions::default().with_unknown([("bar".to_string(), Value::Null)].into()),
        )
        .build()
        .wait_until_workspaces_are_initialized();

    let show_message_params = server.await_notification::<ShowMessage>();

    insta::assert_json_snapshot!(show_message_params, @r#"
    {
      "type": 2,
      "message": "Received unknown options during initialization: 'bar'. Refer to the logs for more details"
    }
    "#);

    Ok(())
}

/// Tests that the server sends a warning notification if user provided unknown options in the
/// workspace configuration.
#[test]
fn unknown_options_in_workspace_configuration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_unknown([("bar".to_string(), Value::Null)].into())),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    let show_message_params = server.await_notification::<ShowMessage>();

    insta::assert_json_snapshot!(show_message_params, @r#"
    {
      "type": 2,
      "message": "Received unknown options for workspace `file://<temp_dir>/foo`: 'bar'. Refer to the logs for more details."
    }
    "#);

    Ok(())
}

/// Tests that the server sends a registration request for the rename capability if the client
/// setting is set to true and dynamic registration is enabled.
#[test]
fn register_rename_capability_when_enabled() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_initialization_options(ClientOptions::default().with_experimental_rename(true))
        .enable_rename_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration, @r#"
    {
      "id": "ty/textDocument/rename",
      "method": "textDocument/rename",
      "registerOptions": {
        "prepareProvider": true
      }
    }
    "#);

    Ok(())
}

/// Tests that rename capability is statically registered during initialization if the client
/// doesn't support dynamic registration, but the server is configured to support it.
#[test]
fn rename_available_without_dynamic_registration() -> Result<()> {
    let workspace_root = SystemPath::new("foo");

    let server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_initialization_options(ClientOptions::default().with_experimental_rename(true))
        .enable_rename_dynamic_registration(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let initialization_result = server.initialization_result().unwrap();
    insta::assert_json_snapshot!(initialization_result.capabilities.rename_provider, @r#"
    {
      "prepareProvider": true
    }
    "#);

    Ok(())
}

/// Tests that the server does not send a registration request for the rename capability if the
/// client setting is set to false and dynamic registration is enabled.
#[test]
fn not_register_rename_capability_when_disabled() -> Result<()> {
    let workspace_root = SystemPath::new("foo");

    TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_initialization_options(ClientOptions::default().with_experimental_rename(false))
        .enable_rename_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    // The `Drop` implementation will make sure that the client did not receive any registration
    // request.

    Ok(())
}

/// Tests that the server can register multiple capabilities at once.
///
/// This test would need to be updated when the server supports additional capabilities in the
/// future.
#[test]
fn register_multiple_capabilities() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_initialization_options(
            ClientOptions::default()
                .with_experimental_rename(true)
                .with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .enable_rename_dynamic_registration(true)
        .enable_diagnostic_dynamic_registration(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, params) = server.await_request::<RegisterCapability>();
    let registrations = params.registrations;

    assert_eq!(registrations.len(), 2);

    insta::assert_json_snapshot!(registrations, @r#"
    [
      {
        "id": "ty/textDocument/diagnostic",
        "method": "textDocument/diagnostic",
        "registerOptions": {
          "documentSelector": null,
          "identifier": "ty",
          "interFileDependencies": true,
          "workDoneProgress": true,
          "workspaceDiagnostics": true
        }
      },
      {
        "id": "ty/textDocument/rename",
        "method": "textDocument/rename",
        "registerOptions": {
          "prepareProvider": true
        }
      }
    ]
    "#);

    Ok(())
}
