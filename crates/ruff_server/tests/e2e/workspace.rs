use anyhow::{Context, Result};
use insta::{assert_json_snapshot, assert_snapshot};

use crate::{TestServer, TestServerBuilder};

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
