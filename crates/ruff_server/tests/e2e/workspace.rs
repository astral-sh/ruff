use std::assert_matches;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use insta::{assert_json_snapshot, assert_snapshot};
use lsp_types::{
    DocumentFormattingParams, DocumentFormattingRequest, DocumentRangeFormattingParams,
    DocumentRangeFormattingRequest, Position, Range, ShowMessageNotification,
    TextDocumentIdentifier, TextEdit,
};
use ruff_server::WorkspaceTrust;
use serde_json::json;
use test_case::test_case;

use crate::{AwaitResponseError, TestServer, TestServerBuilder};

const SOURCE: &str = "value= \"hello\"\n";

#[test]
fn selects_the_correct_workspace_settings_for_multi_root_workspaces() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_workspace("external/Y")?
        .with_file(
            "systemtests/pyproject.toml",
            r#"
[tool.ruff.lint]
ignore = ["F401"]
"#,
        )?
        .with_file("systemtests/tests/common/fakes/wus.py", "import os\n")?
        .with_file("external/Y/wus.py", "import os\n")?
        .build();

    server.open_text_document("systemtests/tests/common/fakes/wus.py", "import os\n", 1);
    server.open_text_document("external/Y/wus.py", "import os\n", 1);

    let diagnostics =
        server.document_diagnostic_request("systemtests/tests/common/fakes/wus.py", None);
    let external_diagnostics = server.document_diagnostic_request("external/Y/wus.py", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "items": [],
      "kind": "full"
    }
    "#
    );

    assert_json_snapshot!(
        external_diagnostics,
        @r#"
    {
      "items": [
        {
          "range": {
            "start": {
              "line": 0,
              "character": 7
            },
            "end": {
              "line": 0,
              "character": 9
            }
          },
          "severity": 2,
          "code": "F401",
          "codeDescription": {
            "href": "https://docs.astral.sh/ruff/rules/unused-import"
          },
          "source": "Ruff",
          "message": "`os` imported but unused\n\nhelp: Remove unused import: `os`",
          "tags": [
            1
          ],
          "data": {
            "code": "F401",
            "edits": [
              {
                "newText": "",
                "range": {
                  "end": {
                    "character": 0,
                    "line": 1
                  },
                  "start": {
                    "character": 0,
                    "line": 0
                  }
                }
              }
            ],
            "is_preferred": true,
            "noqa_edit": {
              "newText": "  # noqa: F401\n",
              "range": {
                "end": {
                  "character": 0,
                  "line": 1
                },
                "start": {
                  "character": 9,
                  "line": 0
                }
              }
            },
            "title": "Remove unused import: `os`"
          }
        }
      ],
      "kind": "full"
    }
    "#
    );

    Ok(())
}

#[test]
fn nested_workspace_root_is_not_excluded_by_an_ancestor() -> Result<()> {
    let mut server = nested_workspace_server(&["sub"], WorkspaceExclusion::Exclude)?;

    assert_snapshot!(
        open_and_format(&mut server, "sub/test.py", SOURCE)
            .context("nested workspace should be formatted")?,
        @"value = 'hello'"
    );
    // Explicitly opening `sub` does not override its own exclusion of `foo`.
    assert!(open_and_format(&mut server, "sub/foo/test.py", SOURCE).is_none());

    Ok(())
}

#[test]
fn nested_workspace_root_is_not_excluded_by_an_ancestor_in_a_multi_root_workspace() -> Result<()> {
    const ISSUE_SOURCE: &str = r#"print("This line is long enough to wrap.")
"#;

    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_workspace("sub")?
        .with_file(
            ".ruff.toml",
            r#"target-version = "py312"
line-length = 40

extend-exclude = [
    "sub",
]
"#,
        )?
        .with_file(
            "sub/.ruff.toml",
            r#"target-version = "py312"
line-length = 40

extend-exclude = [
    "foo",
]
"#,
        )?
        .with_file("test.py", ISSUE_SOURCE)?
        .with_file("sub/test.py", ISSUE_SOURCE)?
        .with_file("sub/foo/test.py", ISSUE_SOURCE)?
        .build();

    assert_snapshot!(
        open_and_format(&mut server, "test.py", ISSUE_SOURCE)
            .context("parent workspace should be formatted")?,
        @r#"
    print(
        "This line is long enough to wrap."
    )
    "#
    );
    assert_snapshot!(
        open_and_format(&mut server, "sub/test.py", ISSUE_SOURCE)
            .context("nested workspace should be formatted")?,
        @r#"
    print(
        "This line is long enough to wrap."
    )
    "#
    );
    assert!(open_and_format(&mut server, "sub/foo/test.py", ISSUE_SOURCE).is_none());

    Ok(())
}

#[test]
fn nested_workspace_remains_excluded_without_explicit_registration() -> Result<()> {
    let mut server = nested_workspace_server(&["."], WorkspaceExclusion::ExtendExclude)?;

    assert!(open_and_format(&mut server, "sub/test.py", SOURCE).is_none());
    assert!(open_and_format(&mut server, "sub/foo/test.py", SOURCE).is_none());

    Ok(())
}

#[test]
fn unrelated_file_outside_workspace_uses_fallback_configuration() -> Result<()> {
    let mut server = nested_workspace_server(&["sub"], WorkspaceExclusion::ExtendExclude)?;

    assert_snapshot!(
        open_and_format(&mut server, "unrelated/test.py", SOURCE)
            .context("unrelated file should use fallback formatting")?,
        @r#"value = "hello""#
    );

    Ok(())
}

#[test]
fn single_file_mode_does_not_index_nested_configuration() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("nested/.ruff.toml", "[format]\nquote-style = \"single\"\n")?
        .with_file("nested/test.py", SOURCE)?
        .with_file("unrelated/test.py", SOURCE)?
        .build();

    assert_snapshot!(
        open_and_format(&mut server, "nested/test.py", SOURCE)
            .context("nested file should use fallback formatting")?,
        @r#"value = "hello""#
    );
    assert_snapshot!(
        open_and_format(&mut server, "unrelated/test.py", SOURCE)
            .context("unrelated file should use fallback formatting")?,
        @r#"value = "hello""#
    );

    Ok(())
}

#[derive(Clone, Copy)]
enum WorkspaceExclusion {
    Exclude,
    ExtendExclude,
}

/// Creates a test server for the following temporary workspace:
///
/// ```text
/// <temp_dir>/
/// ├── .ruff.toml              # exclude or extend-exclude = ["sub"]
/// ├── test.py
/// ├── sub/
/// │   ├── .ruff.toml          # extend-exclude = ["foo"]
/// │   │                       # format.quote-style = "single"
/// │   ├── test.py
/// │   └── foo/
/// │       └── test.py
/// └── unrelated/
///     └── test.py
/// ```
fn nested_workspace_server(
    workspaces: &[&str],
    exclusion: WorkspaceExclusion,
) -> Result<TestServer> {
    let mut builder = TestServerBuilder::new()?;
    for workspace in workspaces {
        builder = builder.with_workspace(workspace)?;
    }

    let server = builder
        .with_file(
            ".ruff.toml",
            match exclusion {
                WorkspaceExclusion::Exclude => "exclude = [\"sub\"]\n",
                WorkspaceExclusion::ExtendExclude => "extend-exclude = [\"sub\"]\n",
            },
        )?
        .with_file(
            "sub/.ruff.toml",
            "extend-exclude = [\"foo\"]\n[format]\nquote-style = \"single\"\n",
        )?
        .with_file("test.py", SOURCE)?
        .with_file("sub/test.py", SOURCE)?
        .with_file("sub/foo/test.py", SOURCE)?
        .with_file("unrelated/test.py", SOURCE)?
        .build();

    Ok(server)
}

fn open_and_format(server: &mut TestServer, path: &str, source: &str) -> Option<String> {
    server.open_text_document(path, source, 1);
    server
        .format_request(path)
        .and_then(|edits| edits.into_iter().next())
        .map(|edit| edit.new_text)
}

#[test]
fn unavailable_document_diagnostic_returns_empty_response() -> Result<()> {
    let mut server = TestServerBuilder::new()?.with_workspace(".")?.build();

    let diagnostics = server.document_diagnostic_request("not-open.py", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "items": [],
      "kind": "full"
    }
    "#
    );

    Ok(())
}

#[test_case(WorkspaceTrust::Trusted; "trusted")]
#[test_case(WorkspaceTrust::Untrusted; "untrusted")]
fn uv_formatting(workspace_trust: WorkspaceTrust) -> Result<()> {
    without_uv(|| {
        let mut server = TestServerBuilder::new()?
            .with_workspace(".")?
            .with_workspace_trust(workspace_trust)
            .with_initialization_options(json!({"settings": {"format": {"backend": "uv"}}}))
            .build();
        server.open_text_document("test.py", "x=1\n", 1);

        let document = server.send_request::<DocumentFormattingRequest>(DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: server.file_uri("test.py"),
            },
            options: Default::default(),
            work_done_progress_params: Default::default(),
        });
        let document = server.try_await_response::<DocumentFormattingRequest>(&document, None);

        let range =
            server.send_request::<DocumentRangeFormattingRequest>(DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: server.file_uri("test.py"),
                },
                range: Range::new(Position::new(0, 0), Position::new(1, 0)),
                options: Default::default(),
                work_done_progress_params: Default::default(),
            });
        let range = server.try_await_response::<DocumentRangeFormattingRequest>(&range, None);

        match workspace_trust {
            WorkspaceTrust::Trusted => {
                assert_matches!(
                    document,
                    Err(AwaitResponseError::RequestFailed(error))
                        if error.message.contains("uv was not found"),
                );
                assert_matches!(
                    range,
                    Err(AwaitResponseError::RequestFailed(error))
                        if error.message.contains("uv was not found"),
                );
                server.await_notification::<ShowMessageNotification>();
                server.await_notification::<ShowMessageNotification>();
            }
            WorkspaceTrust::Untrusted => {
                let expected = Some(vec![TextEdit {
                    range: Range::new(Position::new(0, 0), Position::new(1, 0)),
                    new_text: "x = 1\n".to_string(),
                }]);
                assert_eq!(document?, expected);
                assert_eq!(range?, expected);
            }
        }
        Ok(())
    })
}

/// Isolate PATH from other tests, which run their servers in the same process.
/// Without uv, formatting only succeeds if the internal backend is used.
fn without_uv(body: impl FnOnce() -> Result<()>) -> Result<()> {
    const CHILD: &str = "RUFF_TEST_ISOLATED_CHILD";
    let thread = std::thread::current();
    let name = thread.name().context("missing test name")?;
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return body();
    }

    let empty_path = tempfile::tempdir()?;
    let output = Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, name)
        .env("PATH", empty_path.path())
        .current_dir(empty_path.path())
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // An unmatched `--exact` filter exits successfully without running any tests.
    ensure!(
        output.status.success() && stdout.contains("test result: ok. 1 passed;"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}
