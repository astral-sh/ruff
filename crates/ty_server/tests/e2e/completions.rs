use anyhow::Result;
use lsp_types::Position;
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

    let completions = server.completion_request(&server.file_uri(foo), Position::new(0, 6));

    insta::assert_json_snapshot!(completions, @"[]");

    Ok(())
}

#[test]
fn complete_function_parentheses() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
class Class: ...

class Object:
    def method(self) -> None: ...

object = Object()

def function() -> None: ...

Cla
func
object.met
func(
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default()
                .with_auto_import(false)
                .with_complete_function_parentheses(true),
        )
        .enable_completion_snippets(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let class_completion = callable_completions(&mut server, foo, Position::new(9, 3));
    insta::assert_json_snapshot!(class_completion, @r#"
    [
      {
        "label": "Class",
        "kind": 7,
        "insertText": "Class($0)",
        "insertTextFormat": 2
      }
    ]
    "#);

    let function_completion = callable_completions(&mut server, foo, Position::new(10, 4));
    insta::assert_json_snapshot!(function_completion, @r#"
    [
      {
        "label": "function",
        "kind": 3,
        "insertText": "function($0)",
        "insertTextFormat": 2
      }
    ]
    "#);

    let method_completion = callable_completions(&mut server, foo, Position::new(11, 10));
    insta::assert_json_snapshot!(method_completion, @r#"
    [
      {
        "label": "method",
        "kind": 2,
        "insertText": "method($0)",
        "insertTextFormat": 2
      }
    ]
    "#);

    let before_parenthesis = callable_completions(&mut server, foo, Position::new(12, 4));
    insta::assert_json_snapshot!(before_parenthesis, @r#"
    [
      {
        "label": "function",
        "kind": 3
      }
    ]
    "#);

    Ok(())
}

#[test]
fn complete_function_parentheses_requires_snippet_support() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def function() -> None: ...

func
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default()
                .with_auto_import(false)
                .with_complete_function_parentheses(true),
        )
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completion = callable_completions(&mut server, foo, Position::new(2, 4));
    insta::assert_json_snapshot!(completion, @r#"
    [
      {
        "label": "function",
        "kind": 3
      }
    ]
    "#);

    Ok(())
}

fn callable_completions(
    server: &mut crate::TestServer,
    file: &SystemPath,
    position: Position,
) -> Vec<CallableCompletionSnapshot> {
    let completions = server.completion_request(&server.file_uri(file), position);
    completions
        .into_iter()
        .filter(|completion| matches!(completion.label.as_str(), "Class" | "function" | "method"))
        .map(CallableCompletionSnapshot::from)
        .collect()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CallableCompletionSnapshot {
    label: String,
    kind: Option<lsp_types::CompletionItemKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insert_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insert_text_format: Option<lsp_types::InsertTextFormat>,
}

impl From<lsp_types::CompletionItem> for CallableCompletionSnapshot {
    fn from(completion: lsp_types::CompletionItem) -> Self {
        Self {
            label: completion.label,
            kind: completion.kind,
            insert_text: completion.insert_text,
            insert_text_format: completion.insert_text_format,
        }
    }
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

/// Tests the LSP-facing shape for string-literal completions with an already-typed prefix.
///
/// The server intentionally returns the full completion in `insertText`. Without an explicit
/// `textEdit`, LSP clients are allowed to interpret that insert text relative to the current
/// word; for example, VS Code applies `insertText: "apple"` at `app|` as the suffix `le`.
#[test]
fn string_literal_completion_uses_full_lsp_insert_text() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import Literal
x: Literal[\"apple\"] = \"app\"
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_auto_import(false))
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(1, 26));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "apple",
        "kind": 12,
        "detail": "Literal[\"apple\"]",
        "sortText": "0",
        "insertText": "apple"
      }
    ]
    "#);

    Ok(())
}

#[test]
fn typed_dict_literal_key_completion_before_colon() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import TypedDict

class Box(TypedDict):
    x: float
    y: float
    z: float

def take(box: Box): ...

take({\"\"})
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_auto_import(false))
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(9, 7));

    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "x",
        "kind": 12,
        "detail": "Literal[\"x\"]",
        "sortText": "0",
        "insertText": "x"
      },
      {
        "label": "y",
        "kind": 12,
        "detail": "Literal[\"y\"]",
        "sortText": "1",
        "insertText": "y"
      },
      {
        "label": "z",
        "kind": 12,
        "detail": "Literal[\"z\"]",
        "sortText": "2",
        "insertText": "z"
      }
    ]
    "#);

    Ok(())
}
