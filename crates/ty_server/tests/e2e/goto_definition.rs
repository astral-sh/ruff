use anyhow::Result;
use lsp_types::Position;
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn script_search_paths_resolve_imported_symbols() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let dependency = SystemPath::new("src/dependencies/dependency.py");

    let script = SystemPath::new("src/script.py");
    let script_content = r#"# /// script
# [tool.ty.environment]
# extra-paths = ["./dependencies"]
# ///

from dependency import script_only
"#;

    let ordinary = SystemPath::new("src/ordinary.py");
    let ordinary_content = "from dependency import script_only\n";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(dependency, "def script_only() -> None: ...\n")?
        .with_file(script, script_content)?
        .with_file(ordinary, ordinary_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(script, script_content, 1);
    server.open_text_document(ordinary, ordinary_content, 1);

    let script_definition = server.goto_definition_request(script, Position::new(5, 24));
    insta::assert_json_snapshot!(script_definition, @r#"
    [
      {
        "uri": "file://<temp_dir>/src/dependencies/dependency.py",
        "range": {
          "start": {
            "line": 0,
            "character": 4
          },
          "end": {
            "line": 0,
            "character": 15
          }
        }
      }
    ]
    "#);

    let ordinary_definition = server.goto_definition_request(ordinary, Position::new(0, 24));
    insta::assert_json_snapshot!(ordinary_definition, @"null");

    Ok(())
}

#[cfg(feature = "test-uv")]
mod uv_metadata {
    use anyhow::{Context, Result, anyhow};
    use lsp_types::{
        Code, Definition, DefinitionResponse, FileChangeType, FileEvent, Position,
        TextDocumentContentChangeEvent, TextDocumentContentChangeWholeDocument,
        WorkspaceDocumentDiagnosticReport,
    };
    use ruff_db::system::{SystemPath, SystemPathBuf};
    use ty_project::UseUv;
    use ty_server::{ClientOptions, DiagnosticMode};

    use crate::TestServerBuilder;

    #[test]
    fn synchronization_reports_progress_before_resolving_dependency_definitions() -> Result<()> {
        let workspace_root = SystemPath::new("src");
        let script = SystemPath::new("src/script.py");
        let source = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
from attrs import define
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(workspace_root, None)?
            .with_file(script, source)?
            .with_real_uv(UseUv::Scripts)?
            .enable_work_done_progress(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(script, source, 1);

        let end = server.assert_work_done_progress("Synchronizing scripts")?;
        assert_eq!(
            end.message.as_deref(),
            Some("Finished synchronizing scripts")
        );

        let definition = server
            .goto_definition_request(script, Position::new(4, 18))
            .context("expected attrs.define to resolve")?;
        let DefinitionResponse::Definition(Definition::LocationList(locations)) = definition else {
            return Err(anyhow!("expected dependency definition locations"));
        };
        let location = locations
            .first()
            .context("expected attrs.define definition location")?;
        let path = location
            .uri
            .to_file_path()
            .map_err(|()| anyhow!("expected dependency definition to have a file URI"))?;
        let dependency = SystemPathBuf::from_path_buf(path)
            .map_err(|path| anyhow!("dependency path is not valid UTF-8: {}", path.display()))?;
        let source = std::fs::read_to_string(dependency.as_std_path())?;
        let (line, import) = source
            .lines()
            .enumerate()
            .find(|(_, line)| line.starts_with("from attr import Attribute "))
            .with_context(|| {
                format!("expected attrs to import attr.Attribute in `{dependency}`")
            })?;
        let character = import
            .find("Attribute")
            .context("expected attrs to import Attribute")?;
        let position = Position::new(u32::try_from(line)?, u32::try_from(character)?);

        server.open_text_document(&dependency, &source, 1);
        assert!(
            server
                .goto_definition_request(&dependency, position)
                .is_some(),
            "expected imports inside the dependency to use the script's environment"
        );

        Ok(())
    }

    #[test]
    fn multiple_workspaces_synchronize_independently() -> Result<()> {
        let first_workspace = SystemPath::new("first");
        let first_script = SystemPath::new("first/script.py");
        let first_source = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
from attrs import define
from idna import encode
"#;
        let second_workspace = SystemPath::new("second");
        let second_script = SystemPath::new("second/script.py");
        let second_source = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['idna==3.10']
# ///
from idna import encode
from attrs import define
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(first_workspace, None)?
            .with_workspace(second_workspace, None)?
            .with_file(first_script, first_source)?
            .with_file(second_script, second_source)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(first_script, first_source, 1);
        server.open_text_document(second_script, second_source, 1);
        server.await_diagnostic_refresh();
        server.await_diagnostic_refresh();

        assert!(
            server
                .goto_definition_request(first_script, Position::new(4, 18))
                .is_some(),
            "expected attrs.define to resolve in the first workspace"
        );
        assert!(
            server
                .goto_definition_request(second_script, Position::new(4, 18))
                .is_some(),
            "expected idna.encode to resolve in the second workspace"
        );
        assert!(
            server
                .goto_definition_request(first_script, Position::new(5, 18))
                .is_none(),
            "the first workspace must not resolve the second workspace's dependencies"
        );
        assert!(
            server
                .goto_definition_request(second_script, Position::new(5, 18))
                .is_none(),
            "the second workspace must not resolve the first workspace's dependencies"
        );

        Ok(())
    }

    #[test]
    fn dependencies_resynchronize_after_save() -> Result<()> {
        let workspace_root = SystemPath::new("src");
        let script = SystemPath::new("src/script.py");
        let initial = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
value = 1
"#;
        let updated = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0', 'idna==3.10']
# ///
from idna import encode
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(workspace_root, None)?
            .with_file(script, initial)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(script, initial, 1);
        server.await_diagnostic_refresh();

        server.change_text_document(
            script,
            vec![
                TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                    TextDocumentContentChangeWholeDocument {
                        text: updated.to_string(),
                    },
                ),
            ],
            2,
        );

        assert!(
            server
                .goto_definition_request(script, Position::new(4, 19))
                .is_none(),
            "unsaved dependency changes must keep the previous environment"
        );

        server.write_file(script, updated)?;

        // A watcher notification can arrive after the file is written but before `didSave`.
        // Synchronization must still wait for the save notification.
        server.did_change_watched_files(vec![FileEvent {
            uri: server.file_uri(script),
            kind: FileChangeType::Changed,
        }]);
        server.await_diagnostic_refresh();

        assert!(
            server
                .goto_definition_request(script, Position::new(4, 19))
                .is_none(),
            "watcher events must not synchronize open scripts before they are saved"
        );

        server.save_text_document(script);

        server.await_diagnostic_refresh();

        assert!(
            server
                .goto_definition_request(script, Position::new(4, 19))
                .is_some(),
            "saving must synchronize newly declared script dependencies"
        );

        Ok(())
    }

    #[test]
    fn saving_unsaved_open_metadata_repeats_synchronization() -> Result<()> {
        let script = SystemPath::new("src/script.py");
        let initial = r#"# /// script
# requires-python = '>=3.12'
# dependencies = []
# ///
"#;
        let updated = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
from attrs import define
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(SystemPath::new("src"), None)?
            .with_file(script, initial)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        // Opening synchronizes the backing file, not the unsaved metadata.
        server.open_text_document(script, updated, 1);
        server.await_diagnostic_refresh();
        assert!(
            server
                .goto_definition_request(script, Position::new(4, 18))
                .is_none(),
            "unsaved dependencies must not be installed"
        );

        server.write_file(script, updated)?;
        server.save_text_document(script);
        server.await_diagnostic_refresh();
        assert!(
            server
                .goto_definition_request(script, Position::new(4, 18))
                .is_some(),
            "saving must install attrs even though the editor's metadata is unchanged"
        );

        Ok(())
    }

    #[test]
    fn workspace_check_does_not_synchronize_unsaved_script_metadata() -> Result<()> {
        let script = SystemPath::new("src/script.py");
        let ordinary = "from attrs import define\nprint(define)\n";
        let source = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
from attrs import define
print(define)
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(
                SystemPath::new("src"),
                Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
            )?
            .with_file(script, ordinary)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(script, source, 1);

        // A workspace check must not initialize uv from metadata that exists only in the editor.
        let diagnostics = server.workspace_diagnostic_request(None, None);
        let [WorkspaceDocumentDiagnosticReport::WorkspaceFullDocumentDiagnosticReport(report)] =
            diagnostics.items.as_slice()
        else {
            return Err(anyhow!("expected diagnostics for the unsaved script"));
        };
        let [diagnostic] = report.full_document_diagnostic_report.items.as_slice() else {
            return Err(anyhow!(
                "expected only the unresolved attrs import: {report:?}"
            ));
        };
        assert_eq!(
            diagnostic.code,
            Some(Code::String("unresolved-import".to_string()))
        );

        server.write_file(script, source)?;
        server.save_text_document(script);
        server.await_diagnostic_refresh();
        assert!(
            server
                .goto_definition_request(script, Position::new(4, 18))
                .is_some(),
            "the first save must synchronize the script's dependencies"
        );

        Ok(())
    }

    #[test]
    fn dependencies_resolve_after_invalid_metadata_is_corrected() -> Result<()> {
        let workspace_root = SystemPath::new("src");
        let script = SystemPath::new("src/script.py");
        let initial = r#"# /// script
# requires-python = '>=3.12'
# dependencies = []
# ///
from attrs import define
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(workspace_root, None)?
            .with_file(script, initial)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(script, initial, 1);
        server.await_diagnostic_refresh();

        assert!(
            server
                .goto_definition_request(script, Position::new(4, 18))
                .is_none(),
            "attrs should not resolve before it is declared as a dependency"
        );

        let updates = [
            (
                2,
                r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['']
# ///
from attrs import define
"#,
            ),
            (
                3,
                r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
from attrs import define
"#,
            ),
        ];

        for (version, updated) in updates {
            server.change_text_document(
                script,
                vec![
                    TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                        TextDocumentContentChangeWholeDocument {
                            text: updated.to_string(),
                        },
                    ),
                ],
                version,
            );
            server.write_file(script, updated)?;
            server.save_text_document(script);
            server.await_diagnostic_refresh();
        }

        assert!(
            server
                .goto_definition_request(script, Position::new(4, 18))
                .is_some(),
            "correcting invalid script metadata must make newly installed dependencies available"
        );

        Ok(())
    }

    #[test]
    fn watched_directory_changes_resynchronize_closed_scripts() -> Result<()> {
        let workspace_root = SystemPath::new("src");
        let directory = SystemPath::new("src/scripts");
        let script = SystemPath::new("src/scripts/script.py");
        let initial = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0']
# ///
value = 1
"#;
        let updated = r#"# /// script
# requires-python = '>=3.12'
# dependencies = ['attrs==25.4.0', 'idna==3.10']
# ///
from idna import encode
"#;

        let mut server = TestServerBuilder::new()?
            .with_workspace(workspace_root, None)?
            .with_file(script, initial)?
            .with_real_uv(UseUv::Scripts)?
            .enable_workspace_diagnostic_refresh(true)
            .build()
            .wait_until_workspaces_are_initialized();

        server.open_text_document(script, initial, 1);
        server.await_diagnostic_refresh();
        server.close_text_document(script);
        server.write_file(script, updated)?;

        server.did_change_watched_files(vec![FileEvent {
            uri: server.file_uri(directory),
            kind: FileChangeType::Created,
        }]);

        // Watched-file changes refresh diagnostics immediately and again after uv finishes.
        server.await_diagnostic_refresh();
        server.await_diagnostic_refresh();

        server.open_text_document(script, updated, 2);
        assert!(
            server
                .goto_definition_request(script, Position::new(4, 19))
                .is_some(),
            "watched changes must update environments even while scripts are closed"
        );

        Ok(())
    }
}
