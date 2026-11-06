use anyhow::Result;
use insta::{assert_json_snapshot, assert_snapshot};
use lsp_server::{ErrorCode, Message, RequestId, Response};
use lsp_types::{
    BaseUri, FileChangeType, FileSystemWatcher, GlobPattern, InlayHintRefreshRequest,
    TextDocumentContentChangeEvent, TextDocumentContentChangeWholeDocument,
};
use ruff_db::system::SystemPath;
use ruff_python_trivia::textwrap::dedent;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::diagnostic_snapshots::condensed_document_diagnostic_snapshot;
use crate::{TestServer, TestServerBuilder};

#[test]
fn refreshes_script_dependency_after_rewatch() -> Result<()> {
    let script = SystemPath::new("src/script.py");
    let dependency = SystemPath::new("dependencies/dependency.py");
    let source = dedent(
        r#"
        # /// script
        # [tool.ty.environment]
        # extra-paths = ["../dependencies"]
        # ///
        from dependency import value
        result: int = value
        "#,
    );

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
        )?
        .with_file(script, &source)?
        .with_file(dependency, "value = 1")?
        .with_watched_file_support(true)
        .enable_workspace_diagnostic_refresh(true)
        .enable_inlay_hints(true)
        .enable_inlay_hint_refresh(true)
        .build()
        .wait_until_workspaces_are_initialized();

    // The initial registration covers both the project and the script's external search path.
    let (initial_id, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/dependencies :: **"
    ]
    "#);

    server.open_text_document(script, &source, 1);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(script, None)),
        @""
    );

    // Removing the extra path replaces the registration and leaves the dependency unwatched.
    let without_dependency =
        source.replace(r#"extra-paths = ["../dependencies"]"#, "extra-paths = []");
    server.change_text_document(
        script,
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: without_dependency,
                },
            ),
        ],
        2,
    );
    let (project_id, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **"
    ]
    "#);
    assert_eq!(server.acknowledge_unregistration()?, initial_id);

    // No file event reaches the server for this edit while the dependency is unwatched.
    server.write_file(dependency, "value = 'wrong'")?;
    server.change_text_document(
        script,
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: source.into(),
                },
            ),
        ],
        3,
    );
    let (request_id, _, watches) = server.watcher_registration_request()?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/dependencies :: **"
    ]
    "#);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(script, None)),
        @""
    );

    // Completing registration refreshes the known dependency and finds its missed edit.
    server.acknowledge_request(request_id);
    assert_eq!(server.acknowledge_unregistration()?, project_id);
    server.await_diagnostic_refresh();
    let (refresh_id, ()) = server.await_request::<InlayHintRefreshRequest>();
    server.acknowledge_request(refresh_id);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(script, None)),
        @r#"6:14..6:19[ERROR]: Object of type `Literal["wrong"]` is not assignable to `int`"#
    );
    Ok(())
}

#[test]
fn picks_up_edits_to_unwatched_pth_files() -> Result<()> {
    let main = SystemPath::new("src/main.py");
    let config = SystemPath::new("src/pyproject.toml");
    let source = dedent(
        r"
        from dependency import value
        result: int = value
        ",
    )
    .trim_start()
    .to_owned();
    let project_config = dedent(
        r#"
        [tool.ty.environment]
        python = "../venv"
        "#,
    )
    .trim_start()
    .to_owned();

    let (base_python, pth) = if cfg!(windows) {
        (
            "base/bin/python.exe",
            "venv/Lib/site-packages/dependency.pth",
        )
    } else {
        (
            "base/bin/python",
            "venv/lib/python3.12/site-packages/dependency.pth",
        )
    };

    let builder = TestServerBuilder::new()?.with_workspace(
        SystemPath::new("src"),
        Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
    )?;
    let python_home = builder.file_path("base/bin");
    let old_root = builder.file_path("old");

    let mut server = builder
        .with_file(base_python, "")?
        .with_file(
            "venv/pyvenv.cfg",
            dedent(&format!(
                r"
                home = {python_home}
                version = 3.12
                "
            ))
            .trim_start(),
        )?
        .with_file(pth, old_root.as_str())?
        .with_file("old/dependency.py", "value = 1")?
        .with_file("new/dependency.py", "value = 'wrong'")?
        .with_file(main, &source)?
        .with_file(config, &project_config)?
        .with_watched_file_support(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/venv :: **",
      "file://<temp_dir>/venv/<site-packages> :: **",
      "file://<temp_dir>/old :: **"
    ]
    "#);
    server.open_text_document(main, &source, 1);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(main, None)),
        @""
    );

    // Switch to the default environment so the client stops watching site-packages.
    server.write_file(config, "[tool.ty]")?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(config),
        kind: lsp_types::FileChangeType::Changed,
    }]);
    let (project_only_id, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **"
    ]
    "#);
    server.acknowledge_unregistration()?;

    // The client doesn't report the `.pth` edit while site-packages is unwatched.
    let new = server.file_path("new");
    server.write_file(pth, new.as_str())?;

    // Restoring the environment initially registers paths from the cached `.pth` contents.
    server.write_file(config, project_config)?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(config),
        kind: lsp_types::FileChangeType::Changed,
    }]);
    let (request_id, stale_pth_id, watches) = server.watcher_registration_request()?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/venv :: **",
      "file://<temp_dir>/venv/<site-packages> :: **",
      "file://<temp_dir>/old :: **"
    ]
    "#);
    server.acknowledge_request(request_id);
    assert_eq!(server.acknowledge_unregistration()?, project_only_id);

    // Completing registration refreshes `.pth` and registers its new search path.
    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/venv :: **",
      "file://<temp_dir>/venv/<site-packages> :: **",
      "file://<temp_dir>/new :: **"
    ]
    "#);
    assert_eq!(server.acknowledge_unregistration()?, stale_pth_id);

    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(main, None)),
        @r#"1:14..1:19[ERROR]: Object of type `Literal["wrong"]` is not assignable to `int`"#
    );
    Ok(())
}

/// A single deletion notification for the Pixi directory named by `CONDA_PREFIX` makes ty
/// resolve imports in the open file from the workspace's `.venv`.
#[test]
fn deleting_pixi_prefix_selects_project_venv() -> Result<()> {
    let builder = TestServerBuilder::new()?;
    let workspace = builder.file_path("workspace");
    let prefix = workspace.join(".pixi/envs/dev");
    let venv = workspace.join(".venv");
    let main = workspace.join("main.py");
    let source =
        "from typing import reveal_type\nfrom dependency import origin\nreveal_type(origin)";
    let (python, dependency) = if cfg!(windows) {
        ("python.exe", "Lib/site-packages/dependency.py")
    } else {
        ("bin/python", "lib/python3.13/site-packages/dependency.py")
    };

    let mut server = builder
        .with_workspace(&workspace, None)?
        .with_env_var("CONDA_PREFIX", prefix.as_str())
        .with_files([
            (prefix.join("conda-meta/pixi"), ""),
            (prefix.join(python), ""),
            (prefix.join(dependency), "origin = 'pixi'"),
            (venv.join(dependency), "origin = 'venv'"),
        ])?
        .with_file(
            venv.join("pyvenv.cfg"),
            format!("home = {workspace}\nversion = 3.13\n"),
        )?
        .with_file(&main, source)?
        // Disable relative patterns so a single workspace-wide watch covers both environments.
        .with_watched_file_support(false)
        .build()
        .wait_until_workspaces_are_initialized();

    watcher_registrations(&mut server)?;
    server.open_text_document(&main, source, 1);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(&main, None)),
        @r#"2:12..2:18[INFORMATION]: Revealed type: `Literal["pixi"]`"#
    );

    std::fs::remove_dir_all(&prefix)?;
    server.did_change_watched_file(&prefix, FileChangeType::Deleted);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(&main, None)),
        @r#"2:12..2:18[INFORMATION]: Revealed type: `Literal["venv"]`"#
    );
    Ok(())
}

#[test]
fn nested_search_path_picks_up_missed_dependency_edit() -> Result<()> {
    let main = SystemPath::new("src/main.py");
    let config = SystemPath::new("src/pyproject.toml");
    let dependency = SystemPath::new("dependencies/pkg/dependency.py");
    let source = dedent(
        r"
        from pkg.dependency import value
        result: int = value
        ",
    )
    .trim_start()
    .to_owned();

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
        )?
        .with_file(main, &source)?
        .with_file(
            config,
            dedent(
                r#"
                [tool.ty.environment]
                extra-paths = ["../dependencies"]
                "#,
            )
            .trim_start(),
        )?
        .with_file(dependency, "value = 1")?
        .with_watched_file_support(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/dependencies :: **"
    ]
    "#);
    server.open_text_document(main, &source, 1);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(main, None)),
        @""
    );

    // Remove the parent search path before changing the dependency on disk.
    server.write_file(config, "[tool.ty]")?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(config),
        kind: lsp_types::FileChangeType::Changed,
    }]);
    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **"
    ]
    "#);
    server.acknowledge_unregistration()?;

    // No file event reports the edit while the parent search path is unwatched.
    server.write_file(dependency, "value = 'wrong'")?;
    // Register the nested search path, which contains the previously read dependency.
    server.write_file(
        config,
        dedent(
            r#"
            [tool.ty.environment]
            extra-paths = ["../dependencies/pkg"]
            "#,
        )
        .trim_start(),
    )?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(config),
        kind: lsp_types::FileChangeType::Changed,
    }]);
    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "file://<temp_dir>/src :: **",
      "file://<temp_dir>/dependencies/pkg :: **"
    ]
    "#);
    server.acknowledge_unregistration()?;

    // The new search root exposes the same file as `dependency` instead of `pkg.dependency`.
    // Reimport it to check that the edit missed while it was unwatched has been refreshed.
    server.change_text_document(
        main,
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: dedent(
                        r"
                        from dependency import value
                        result: int = value
                        ",
                    )
                    .trim_start()
                    .to_owned(),
                },
            ),
        ],
        2,
    );
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(main, None)),
        @r#"1:14..1:19[ERROR]: Object of type `Literal["wrong"]` is not assignable to `int`"#
    );
    Ok(())
}

#[test]
fn watches_project_without_relative_patterns() -> Result<()> {
    let main = SystemPath::new("src/main.py");
    let dependency = SystemPath::new("src/dependency.py");
    let source = dedent(
        r"
        from dependency import value
        result: int = value
        ",
    )
    .trim_start()
    .to_owned();

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace)),
        )?
        .with_file(main, &source)?
        .with_file(dependency, "value = 1")?
        .with_watched_file_support(false)
        .build()
        .wait_until_workspaces_are_initialized();

    // A client without relative patterns receives one workspace-wide glob.
    let (_, watches) = watcher_registrations(&mut server)?;
    assert_json_snapshot!(watcher_snapshot(&watches), @r#"
    [
      "**"
    ]
    "#);

    server.open_text_document(main, &source, 1);
    server.write_file(dependency, "value = 'wrong'")?;
    server.did_change_watched_files(vec![lsp_types::FileEvent {
        uri: server.file_uri(dependency),
        kind: lsp_types::FileChangeType::Changed,
    }]);
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request(main, None)),
        @r#"1:14..1:19[ERROR]: Object of type `Literal["wrong"]` is not assignable to `int`"#
    );
    Ok(())
}

#[test]
fn falls_back_to_a_workspace_watch_after_registration_fails() -> Result<()> {
    let main = SystemPath::new("src/main.py");
    let config = SystemPath::new("src/ty.toml");
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(main, "x = 1")?
        .with_file(config, "")?
        .with_file("deps/dep.py", "")?
        .with_watched_file_support(true)
        .build()
        .wait_until_workspaces_are_initialized();
    let (initial_id, _) = watcher_registrations(&mut server)?;

    server.write_file(config, r#"environment.extra-paths = ["../deps"]"#)?;
    server.did_change_watched_file(config, FileChangeType::Changed);
    let (request, _, watches) = server.watcher_registration_request()?;
    assert_eq!(watches.len(), 2);
    reject_registration(&mut server, request);

    let (fallback, _, watches) = server.watcher_registration_request()?;
    assert_eq!(watcher_snapshot(&watches), ["**"]);
    server.acknowledge_request(fallback);
    assert_eq!(server.acknowledge_unregistration()?, initial_id);

    server.open_text_document(main, "x = 1", 1);
    server.write_file(config, "")?;
    server.did_change_watched_file(config, FileChangeType::Changed);
    server.document_diagnostic_request(main, None);
    server.assert_no_pending_messages();
    Ok(())
}

#[test]
fn stops_registering_after_workspace_watch_is_rejected() -> Result<()> {
    let main = SystemPath::new("src/main.py");
    let config = SystemPath::new("src/ty.toml");
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(main, "x = 1")?
        .with_file(config, "")?
        .with_file("deps/dep.py", "")?
        .with_watched_file_support(true)
        .build()
        .wait_until_workspaces_are_initialized();
    watcher_registrations(&mut server)?;

    server.write_file(config, r#"environment.extra-paths = ["../deps"]"#)?;
    server.did_change_watched_file(config, FileChangeType::Changed);
    let (request, _, watches) = server.watcher_registration_request()?;
    assert_eq!(watches.len(), 2);
    reject_registration(&mut server, request);

    let (fallback, _, watches) = server.watcher_registration_request()?;
    assert_eq!(watcher_snapshot(&watches), ["**"]);
    reject_registration(&mut server, fallback);

    server.open_text_document(main, "x = 1", 1);
    server.write_file(config, "")?;
    server.did_change_watched_file(config, FileChangeType::Changed);
    server.document_diagnostic_request(main, None);
    server.assert_no_pending_messages();
    Ok(())
}

fn reject_registration(server: &mut TestServer, request: RequestId) {
    server.send(Message::Response(Response::new_err(
        request,
        ErrorCode::InternalError as i32,
        "Watch limit reached".to_owned(),
    )));
}

fn watcher_registrations(server: &mut TestServer) -> Result<(String, Vec<FileSystemWatcher>)> {
    let (request_id, registration_id, watchers) = server.watcher_registration_request()?;
    server.acknowledge_request(request_id);
    Ok((registration_id, watchers))
}

fn watcher_snapshot(watchers: &[FileSystemWatcher]) -> Vec<String> {
    watchers
        .iter()
        .map(|watcher| {
            let pattern = match &watcher.glob_pattern {
                GlobPattern::Pattern(pattern) => pattern.clone(),
                GlobPattern::RelativePattern(relative) => match &relative.base_uri {
                    BaseUri::Uri(uri) => format!("{uri} :: {}", relative.pattern),
                    BaseUri::WorkspaceFolder(folder) => {
                        format!("workspace {} :: {}", folder.uri, relative.pattern)
                    }
                },
            };
            // The fixture's virtual environment has a different site-packages layout on Windows.
            let pattern = pattern
                .replace("/venv/Lib/site-packages", "/venv/<site-packages>")
                .replace(
                    "/venv/lib/python3.12/site-packages",
                    "/venv/<site-packages>",
                );
            if let Some(kind) = watcher.kind {
                format!("{pattern} [{kind:?}]")
            } else {
                pattern
            }
        })
        .collect()
}
