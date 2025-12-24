use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

/// Tests that auto-import is enabled by default.
#[test]
fn default_auto_import() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
walktr
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let completions = server.completion_request(&server.file_uri(foo), Position::new(0, 6));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "walktree (import inspect)",
        "kind": 3,
        "sortText": "0",
        "insertText": "walktree",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from inspect import walktree\n"
          }
        ]
      }
    ]
    "#);

    Ok(())
}

/// Tests that disabling auto-import works.
#[test]
fn disable_auto_import() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
walktr
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_auto_import(false))
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let completions = server.completion_request(&server.file_uri(foo), Position::new(0, 6));

    insta::assert_json_snapshot!(completions, @"[]");

    Ok(())
}

/// Tests that auto-import completions show the fully
/// qualified form when it will insert it for you. Also,
/// that an `import` won't be shown when it won't
/// actually be inserted.
#[test]
fn auto_import_shows_qualification() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import typing

TypedDi<CURSOR>
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let completions = server.completion_request(&server.file_uri(foo), Position::new(2, 7));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "typing.TypedDict",
        "kind": 6,
        "sortText": "0",
        "insertText": "typing.TypedDict"
      },
      {
        "label": "typing.is_typeddict",
        "kind": 3,
        "sortText": "1",
        "insertText": "typing.is_typeddict"
      },
      {
        "label": "_FilterConfigurationTypedDict (import logging.config)",
        "kind": 7,
        "sortText": "2",
        "insertText": "_FilterConfigurationTypedDict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from logging.config import _FilterConfigurationTypedDict\n"
          }
        ]
      },
      {
        "label": "_FormatterConfigurationTypedDict (import logging.config)",
        "kind": 6,
        "sortText": "3",
        "insertText": "_FormatterConfigurationTypedDict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from logging.config import _FormatterConfigurationTypedDict\n"
          }
        ]
      }
    ]
    "#);

    Ok(())
}

/// Tests that auto-import completions show the fully
/// qualified form when it will insert it for you *and*
/// will also show the import when it will be inserted.
#[test]
fn auto_import_shows_qualification_and_import() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
TypedDi<CURSOR>
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let completions = server.completion_request(&server.file_uri(foo), Position::new(0, 7));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "TypedDict (import typing)",
        "kind": 6,
        "sortText": "0",
        "insertText": "TypedDict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from typing import TypedDict\n"
          }
        ]
      },
      {
        "label": "is_typeddict (import typing)",
        "kind": 3,
        "sortText": "1",
        "insertText": "is_typeddict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from typing import is_typeddict\n"
          }
        ]
      },
      {
        "label": "_FilterConfigurationTypedDict (import logging.config)",
        "kind": 7,
        "sortText": "2",
        "insertText": "_FilterConfigurationTypedDict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from logging.config import _FilterConfigurationTypedDict\n"
          }
        ]
      },
      {
        "label": "_FormatterConfigurationTypedDict (import logging.config)",
        "kind": 6,
        "sortText": "3",
        "insertText": "_FormatterConfigurationTypedDict",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from logging.config import _FormatterConfigurationTypedDict\n"
          }
        ]
      }
    ]
    "#);

    Ok(())
}

/// Tests that completions for function arguments will
/// show a `=` suffix.
#[test]
fn function_parameter_shows_equals_suffix() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import re
re.match('', '', fla<CURSOR>
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_auto_import(false))
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let completions = server.completion_request(&server.file_uri(foo), Position::new(1, 20));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "flags=",
        "kind": 6,
        "detail": "int",
        "sortText": "0",
        "insertText": "flags="
      },
      {
        "label": "FloatingPointError",
        "kind": 7,
        "detail": "<class 'FloatingPointError'>",
        "documentation": {
          "kind": "plaintext",
          "value": "Floating-point operation failed.\n"
        },
        "sortText": "1"
      },
      {
        "label": "PythonFinalizationError",
        "kind": 7,
        "detail": "<class 'PythonFinalizationError'>",
        "documentation": {
          "kind": "plaintext",
          "value": "Operation blocked during Python finalization.\n"
        },
        "sortText": "2"
      },
      {
        "label": "float",
        "kind": 7,
        "detail": "<class 'float'>",
        "documentation": {
          "kind": "plaintext",
          "value": "Convert a string or number to a floating-point number, if possible.\n"
        },
        "sortText": "3"
      }
    ]
    "#);

    Ok(())
}
