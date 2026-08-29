use anyhow::Result;
use insta::assert_snapshot;
use lsp_types::{
    DidOpenTextDocumentNotification, DidOpenTextDocumentParams, LanguageKind, TextDocumentItem,
};
use ruff_db::system::SystemPath;
use ruff_python_trivia::textwrap::dedent;
use ty_project::UseUv;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::TestServerBuilder;
use crate::workspace_folders::condensed_workspace_diagnostic_snapshot;

#[test]
#[cfg(feature = "test-uv")]
fn closed_scripts_are_prepared_at_startup() -> Result<()> {
    let ordinary = SystemPath::new("src/main.py");
    let script = SystemPath::new("src/script.py");
    let mut server = workspace_builder()?
        .with_file(ordinary, "ordinary_missing")?
        .with_file(
            script,
            dedent(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = []
                # ///
                missing
                "#,
            ),
        )?
        .with_real_uv(UseUv::Scripts)?
        .enable_workspace_diagnostic_refresh(true)
        .build()
        .wait_until_workspaces_are_initialized();

    // Closed scripts are initialized without a document-open notification.
    server.await_diagnostic_refresh();
    let report = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(condensed_workspace_diagnostic_snapshot(report), @"
    file://<temp_dir>/src/main.py
    	0:0..0:16[ERROR]: Name `ordinary_missing` used when not defined
    file://<temp_dir>/src/script.py
    	5:0..5:7[ERROR]: Name `missing` used when not defined
    ");
    Ok(())
}

#[test]
#[cfg(feature = "test-uv")]
fn created_closed_script_is_prepared_after_a_file_event() -> Result<()> {
    let script = SystemPath::new("src/script.py");
    let mut server = workspace_builder()?
        .with_real_uv(UseUv::Scripts)?
        .enable_workspace_diagnostic_refresh(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.write_file(
        script,
        dedent(
            r#"
            # /// script
            # requires-python = ">=3.12"
            # dependencies = []
            # ///
            missing
            "#,
        ),
    )?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(script),
        kind: lsp_types::FileChangeType::Created,
    }]);

    // The file event and completed synchronization each refresh diagnostics.
    server.await_diagnostic_refresh();
    server.await_diagnostic_refresh();
    let report = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(condensed_workspace_diagnostic_snapshot(report), @"
    file://<temp_dir>/src/script.py
    	5:0..5:7[ERROR]: Name `missing` used when not defined
    ");
    Ok(())
}

#[test]
fn exclude_scripts_uses_saved_contents_after_close() -> Result<()> {
    let script = SystemPath::new("src/script.py");
    let ordinary = SystemPath::new("src/main.py");
    let unsaved = dedent(
        r"
        # /// script
        # dependencies = []
        # ///
        unsaved_missing
        ",
    );
    let mut server = workspace_builder()?
        .with_file(
            "src/ty.toml",
            r"
            [src]
            exclude-scripts = true
            ",
        )?
        .with_file(script, "saved_missing\n")?
        .with_file(ordinary, "ordinary_missing\n")?
        .with_use_uv(UseUv::Off)
        .build()
        .wait_until_workspaces_are_initialized();

    // `exclude-scripts` omits this file from workspace diagnostics because its unsaved
    // contents contain a PEP 723 block.
    server.open_text_document(script, unsaved, 1);

    let report = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(condensed_workspace_diagnostic_snapshot(report), @"
    file://<temp_dir>/src/main.py
    	0:0..0:16[ERROR]: Name `ordinary_missing` used when not defined
    ");

    // The saved file has no script metadata, so closing without saving includes it in
    // workspace diagnostics again.
    server.close_text_document(script);

    let report = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(condensed_workspace_diagnostic_snapshot(report), @"
    file://<temp_dir>/src/main.py
    	0:0..0:16[ERROR]: Name `ordinary_missing` used when not defined
    file://<temp_dir>/src/script.py
    	0:0..0:13[ERROR]: Name `saved_missing` used when not defined
    ");
    Ok(())
}

#[test]
fn non_python_overlay_does_not_block_workspace_diagnostics() -> Result<()> {
    let script = SystemPath::new("src/script.py");
    let main = SystemPath::new("src/main.py");
    let mut server = workspace_builder()?
        .with_file(script, "pass\n")?
        .with_file(main, "missing\n")?
        .with_use_uv(UseUv::Scripts)
        .with_env_var("UV", "missing-script-preparation-uv")
        .enable_workspace_diagnostic_refresh(true)
        .build()
        .wait_until_workspaces_are_initialized();
    // This document has an editor overlay, but is not in the diagnostic open-file set.
    server.send_notification::<DidOpenTextDocumentNotification>(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: server.file_uri(script),
            language_id: LanguageKind::Plaintext,
            version: 1,
            text: dedent(
                r"
                # /// script
                # dependencies = []
                # ///
                pass
                ",
            )
            .into_owned(),
        },
    });

    let report = server.workspace_diagnostic_request(None, None);
    assert_snapshot!(condensed_workspace_diagnostic_snapshot(report), @"
    file://<temp_dir>/src/main.py
    	0:0..0:7[ERROR]: Name `missing` used when not defined
    ");
    Ok(())
}

fn workspace_builder() -> Result<TestServerBuilder> {
    TestServerBuilder::new()?.with_workspace(
        SystemPath::new("src"),
        Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
    )
}
