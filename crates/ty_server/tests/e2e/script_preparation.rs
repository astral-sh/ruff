use anyhow::Result;
use insta::assert_snapshot;
use ruff_db::system::SystemPath;
use ruff_python_trivia::textwrap::dedent;
use ty_project::UseUv;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::TestServerBuilder;
use crate::workspace_folders::condensed_workspace_diagnostic_snapshot;

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

fn workspace_builder() -> Result<TestServerBuilder> {
    TestServerBuilder::new()?.with_workspace(
        SystemPath::new("src"),
        Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
    )
}
