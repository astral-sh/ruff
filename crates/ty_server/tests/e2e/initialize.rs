use anyhow::Result;
use lsp_types::request::RegisterCapability;
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
        .with_workspace(workspace_root, None)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    let initialization_result = server.initialization_result().unwrap();

    insta::assert_json_snapshot!("initialization_with_workspace", initialization_result);

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
