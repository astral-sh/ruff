use anyhow::Result;
use lsp_types::{DiagnosticServerCapabilities, request::RegisterCapability};
use ruff_db::system::SystemPath;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::TestServerBuilder;

#[test]
fn empty_workspace_folders() -> Result<()> {
    let server = TestServerBuilder::new()?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    let initialization_result = server.initialization_result().unwrap();

    insta::assert_json_snapshot!("initialization", initialization_result);

    Ok(())
}

#[test]
fn single_workspace_folder() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let server = TestServerBuilder::new()?
        .with_workspace(workspace_root, ClientOptions::default())?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    let initialization_result = server.initialization_result().unwrap();

    insta::assert_json_snapshot!("initialization_with_workspace", initialization_result);

    Ok(())
}

/// Tests that the server sends a registration request for diagnostics if workspace diagnostics
/// are enabled and dynamic registration is enabled.
#[test]
fn workspace_diagnostic_registration_enable() -> Result<()> {
    let workspace_root = SystemPath::new("foo");
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )?
        .enable_diagnostic_dynamic_registration(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    let (_, params) = server.await_request::<RegisterCapability>()?;
    let [registration] = params.registrations.as_slice() else {
        panic!(
            "Expected a single registration, got: {:#?}",
            params.registrations
        );
    };

    insta::assert_json_snapshot!(registration);

    Ok(())
}

/// Tests that the server does *not* send a registration request if workspace diagnostics
/// are disabled, even if dynamic registration is enabled.
#[test]
fn workspace_diagnostic_registration_disable() -> Result<()> {
    let workspace_root = SystemPath::new("foo");

    // The `Drop` implementation would assert that the no requests were sent by the server.
    let server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::OpenFilesOnly),
        )?
        .enable_diagnostic_dynamic_registration(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    let diagnostic_capabilities = server
        .initialization_result()
        .unwrap()
        .capabilities
        .diagnostic_provider
        .as_ref()
        .unwrap();

    let DiagnosticServerCapabilities::Options(options) = diagnostic_capabilities else {
        panic!("Expected diagnostic capabilities to be options, got: {diagnostic_capabilities:#?}");
    };

    assert!(
        !options.workspace_diagnostics,
        "Expected workspace diagnostics to be disabled"
    );

    Ok(())
}
