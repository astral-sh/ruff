use anyhow::Result;
use insta::assert_snapshot;
use lsp_types::{
    DiagnosticSeverity, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Position, WorkspaceDiagnosticReport,
    WorkspaceDiagnosticReportPartialResult, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport, notification::PublishDiagnostics,
};
use ruff_db::system::SystemPath;
use ty_server::{ClientOptions, DiagnosticMode, GlobalOptions, WorkspaceOptions};

use crate::{
    TestServer, TestServerBuilder,
    pull_diagnostics::{
        assert_workspace_diagnostics_suspends_for_long_polling, send_workspace_diagnostic_request,
        shutdown_and_await_workspace_diagnostic,
    },
};

/// Test that we can initialize multiple workspace folders.
#[test]
fn initialize_multiple_workspace_folders() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .with_workspace(root2, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that we can add a workspace folder after the server
/// is initialized.
#[test]
fn add_workspace_folder_after_init() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    // Take an initial snapshot of diagnostics to confirm that we
    // don't see `root2/main.py`.
    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    server.add_workspace_folder(root2, None)?;
    server.change_workspace_folders([root2], []);
    server = server.wait_until_workspaces_are_initialized();

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that we can add multiple workspace folders simultaneously.
#[test]
fn add_multiple_workspace_folders() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let root3 = SystemPath::new("root3");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_file(root3.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    // Take an initial snapshot of diagnostics to confirm that we
    // don't see `root2/main.py` or `root3/main.py`.
    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    server.add_workspace_folder(root2, None)?;
    server.add_workspace_folder(root3, None)?;
    server.change_workspace_folders([root2, root3], []);
    server = server.wait_until_workspaces_are_initialized();

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root3/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that we can remove a workspace folder after the server
/// is initialized.
#[test]
fn remove_workspace_folder_after_init() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .with_workspace(root2, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    // Assert that we get diagnostics across both workspaces
    // initially.
    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    // Now remove one of the workspaces and assert that the
    // diagnostics are now limited only to `root1`.
    server.change_workspace_folders([], [root2]);
    // We don't need to wait for workspace initialization
    // since we are only removing a workspace. That is, the
    // server is not expected to send a `workspace/configuration`
    // request.

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that we can remove multiple workspace folders simultaneously.
#[test]
fn remove_multiple_workspace_folders() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let root3 = SystemPath::new("root3");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_file(root3.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .with_workspace(root2, None)?
        .with_workspace(root3, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    // Assert that we get diagnostics across all workspaces
    // initially.
    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root3/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    // Now remove two of the workspaces and assert that the
    // diagnostics are now limited only to `root1`.
    server.change_workspace_folders([], [root2, root3]);
    // We don't need to wait for workspace initialization
    // since we are only removing a workspace. That is, the
    // server is not expected to send a `workspace/configuration`
    // request.

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that we can remove a workspace folder even while there
/// is an open document from that folder.
#[test]
fn remove_workspace_folder_with_open_document() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let main1 = root1.join("main.py");
    let main2 = root2.join("main.py");
    let main1_content = "does_not_exist1()";
    let main2_content = "does_not_exist2()";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_file(&main1, main1_content)?
        .with_file(&main2, main1_content)?
        .with_workspace(root1, None)?
        .with_workspace(root2, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(&main1, main1_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:0..0:15[ERROR]: Name `does_not_exist1` used when not defined",
    );

    server.open_text_document(&main2, main2_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main2, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:0..0:15[ERROR]: Name `does_not_exist2` used when not defined",
    );

    server.change_workspace_folders([], [root2]);
    let _ = server.await_notification::<PublishDiagnostics>();

    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:0..0:15[ERROR]: Name `does_not_exist1` used when not defined",
    );

    let document_diagnostics = server.document_diagnostic_request(&main2, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"",
    );

    Ok(())
}

/// Tests that we can add and remove workspace folders at the same time.
#[test]
fn add_and_remove_workspace_folders() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let root3 = SystemPath::new("root3");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_file(root2.join("main.py"), "does_not_exist()")?
        .with_file(root3.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .with_workspace(root2, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    // Take an initial snapshot of diagnostics to confirm that we
    // don't see `root3/main.py`.
    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root2/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    server.add_workspace_folder(root3, None)?;
    server.change_workspace_folders([root3], [root2]);
    // root3 needs to be initialized, so we expect the server
    // to send a `workspace/configuration` request.
    server = server.wait_until_workspaces_are_initialized();

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    file://<temp_dir>/root3/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that if we add a workspace folder that has already been
/// added, then it's a no-op and things still work.
#[test]
fn add_existing_workspace_folder_is_no_op() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.change_workspace_folders([root1], []);
    // Since `root1` is already a workspace folder, the
    // server won't attempt to re-request configuration.
    // Thus, we don't need to wait for that request here.
    // Arguably, this is debatable. Re-adding a workspace
    // folder that already exists is perhaps a signal that
    // we *should* re-request configuration. But this test
    // is merely asserting current behavior and that we are
    // thoughtful about changing it.

    let workspace_diagnostics = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @r"
    file://<temp_dir>/root1/main.py
    	0:0..0:14[ERROR]: Name `does_not_exist` used when not defined
    "
    );

    Ok(())
}

/// Tests that if we add a workspace folder that has already been
/// added, then it's a no-op and things still work.
#[test]
fn remove_only_workspace() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(root1.join("main.py"), "does_not_exist()")?
        .with_workspace(root1, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.change_workspace_folders([], [root1]);

    let workspace_diagnostics = get_expected_empty_workspace_diagnostics_and_shutdown(server);
    assert_snapshot!(
        condensed_workspace_diagnostic_snapshot(workspace_diagnostics), @""
    );

    Ok(())
}

/// Test that we can have different settings for each workspace folder.
#[test]
fn different_settings() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let main1 = root1.join("main.py");
    let main2 = root2.join("main.py");
    let main_content = "ZQZQZQ = None\nZQZQ";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_file(&main1, main_content)?
        .with_file(&main2, main_content)?
        .with_workspace(root1, None)?
        // We disable language services in the second workspace
        // folder. Below, we assert that completions work in `root1`
        // but not `root2`.
        .with_workspace(
            root2,
            Some(ClientOptions {
                workspace: WorkspaceOptions {
                    disable_language_services: Some(true),
                    ..WorkspaceOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(&main1, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let completions = server.completion_request(&server.file_uri(&main1), Position::new(1, 4));
    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "ZQZQZQ",
        "kind": 22,
        "detail": "None",
        "documentation": {
          "kind": "plaintext",
          "value": "The type of the None singleton.\n"
        },
        "sortText": "0"
      }
    ]
    "#);

    server.open_text_document(&main2, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let completions = server.completion_request(&server.file_uri(&main2), Position::new(1, 4));
    insta::assert_json_snapshot!(completions, @"[]");

    Ok(())
}

/// Test that workspace folders cannot realistically have different
/// global settings.
///
/// Note that this scenario isn't currently possible in VS Code as of
/// 2026-01-30 because the global settings are marked as scoped to the
/// "window," which will prevent VS Code from sending different values
/// to each workspace folder. But, other non-VS Code clients might
/// do something different, including allowing workspace folders to
/// purportedly have different global setting values. (Where "global"
/// here is referring the ty server's idea of what ought to be global
/// and not necessarily according to the LSP protocol or the LSP
/// clients.)
#[test]
fn global_settings_precedence() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let main1 = root1.join("main.py");
    let main2 = root2.join("main.py");
    let main_content = "(";

    // This puts the setting change (no syntax errors) on root2,
    // which causes it to take precedence and apply even to root1.

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_file(&main1, main_content)?
        .with_file(&main2, main_content)?
        .with_workspace(root1, None)?
        .with_workspace(
            root2,
            Some(ClientOptions {
                global: GlobalOptions {
                    show_syntax_errors: Some(false),
                    ..GlobalOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(&main1, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"",
    );

    server.open_text_document(&main2, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main2, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"",
    );

    // Now we do it again, but apply the settings to root1 which
    // comes before root2. The default settings on root2 end up
    // winning out, and we get syntax error diagnostics.

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_file(&main1, main_content)?
        .with_file(&main2, main_content)?
        .with_workspace(
            root1,
            Some(ClientOptions {
                global: GlobalOptions {
                    show_syntax_errors: Some(false),
                    ..GlobalOptions::default()
                },
                ..ClientOptions::default()
            }),
        )?
        .with_workspace(root2, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(&main1, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:1..0:1[ERROR]: unexpected EOF while parsing",
    );

    server.open_text_document(&main2, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main2, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:1..0:1[ERROR]: unexpected EOF while parsing",
    );

    Ok(())
}

/// Test that workspace folders can be a vehicle for a change
/// to global settings. And that when global settings are
/// changed, it applies to all workspace folders.
#[test]
fn global_settings_change() -> Result<()> {
    let root1 = SystemPath::new("root1");
    let root2 = SystemPath::new("root2");
    let main1 = root1.join("main.py");
    let main2 = root2.join("main.py");
    let main_content = "(";

    // We initialize with default settings, which means
    // we get syntax error diagnostics.

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_file(&main1, main_content)?
        .with_file(&main2, main_content)?
        .with_workspace(root1, None)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(&main1, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"0:1..0:1[ERROR]: unexpected EOF while parsing",
    );

    // Now we'll add a new workspace folder with syntax error
    // diagnostics disabled. This will apply not just to the
    // new folder, but to the existing folders.
    server.add_workspace_folder(
        root2,
        Some(ClientOptions {
            global: GlobalOptions {
                show_syntax_errors: Some(false),
                ..GlobalOptions::default()
            },
            ..ClientOptions::default()
        }),
    )?;
    server.change_workspace_folders([root2], []);
    server = server.wait_until_workspaces_are_initialized();

    let document_diagnostics = server.document_diagnostic_request(&main1, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"",
    );

    server.open_text_document(&main2, main_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();
    let document_diagnostics = server.document_diagnostic_request(&main2, None);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(document_diagnostics),
        @"",
    );

    Ok(())
}

/// A helper routine for creating a snapshot for a collection of
/// workspace diagnostics.
///
/// We mostly use this in our workspace folder tests to check that the
/// LSP is correctly recognizing and reporting diagnostics for each
/// workspace folder. This isn't really meant to test the diagnostics
/// themselves, hence the condensed output.
fn condensed_workspace_diagnostic_snapshot(report: WorkspaceDiagnosticReportResult) -> String {
    let items = match report {
        WorkspaceDiagnosticReportResult::Report(WorkspaceDiagnosticReport { items }) => items,
        WorkspaceDiagnosticReportResult::Partial(WorkspaceDiagnosticReportPartialResult {
            items,
        }) => items,
    };
    items
        .into_iter()
        .map(|item| match item {
            WorkspaceDocumentDiagnosticReport::Full(doc_report) => {
                let diagnostics = condensed_full_document_diagnostic_report(
                    doc_report.full_document_diagnostic_report,
                )
                .join("\n\t");
                format!("{}\n\t{diagnostics}", doc_report.uri)
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(doc_report) => {
                format!("{}\n\tUNCHANGED", doc_report.uri)
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn condensed_document_diagnostic_snapshot(report: DocumentDiagnosticReportResult) -> String {
    match report {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(full)) => {
            condensed_full_document_diagnostic_report(full.full_document_diagnostic_report)
                .join("\n")
        }
        // NOTE: It might be worth providing more details for these
        // cases, but I don't think there's currently a use case for
        // it.
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(_)) => {
            "UNCHANGED".to_string()
        }
        DocumentDiagnosticReportResult::Partial(_) => "PARTIAL".to_string(),
    }
}

fn condensed_full_document_diagnostic_report(report: FullDocumentDiagnosticReport) -> Vec<String> {
    report
        .items
        .into_iter()
        .map(|d| {
            let range = format!(
                "{start_line}:{start_char}..{end_line}:{end_char}",
                start_line = d.range.start.line,
                start_char = d.range.start.character,
                end_line = d.range.end.line,
                end_char = d.range.end.character,
            );
            let severity = match d.severity {
                Some(DiagnosticSeverity::ERROR) => "ERROR",
                Some(DiagnosticSeverity::WARNING) => "WARNING",
                Some(DiagnosticSeverity::INFORMATION) => "INFORMATION",
                Some(DiagnosticSeverity::HINT) => "HINT",
                None | Some(_) => "unknown",
            };
            format!("{range}[{severity}]: {message}", message = d.message,)
        })
        .collect()
}

/// Asks for workspace diagnostics in a way that anticipates "long polling."
///
/// This specifically occurs when there aren't any workspace diagnostics
/// to report. We use this technique in some (weird) tests where there aren't
/// any workspaces remaining, and thus expect to not receive any diagnostics.
///
/// This also initiates a shutdown of the server, which ultimately cancels
/// the long polling and returns (an expected empty) workspace diagnostic
/// response.
///
/// NOTE: Using this sparingly, since the way this asserts that long polling
/// is occurring is by sending a request with a 2-second timeout that we
/// expect to never have a response for.
fn get_expected_empty_workspace_diagnostics_and_shutdown(
    mut server: TestServer,
) -> WorkspaceDiagnosticReportResult {
    let request_id = send_workspace_diagnostic_request(&mut server);
    assert_workspace_diagnostics_suspends_for_long_polling(&mut server, &request_id);
    shutdown_and_await_workspace_diagnostic(server, &request_id)
}
