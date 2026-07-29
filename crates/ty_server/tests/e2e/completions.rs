use anyhow::Result;
use lsp_types::{Documentation, MarkupKind, Position};
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
fn complete_function_parentheses_disabled_by_default() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def complete_parentheses() -> None: ...

complete_parenth
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .enable_completion_snippets(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(2, 16));
    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "complete_parentheses",
        "kind": 3,
        "detail": "def complete_parentheses() -> None",
        "sortText": "0"
      }
    ]
    "#);

    Ok(())
}

#[test]
fn complete_function_parentheses() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def complete_parentheses() -> None: ...

complete_parenth
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_complete_function_parentheses(true),
        )
        .enable_completion_snippets(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(2, 16));
    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "complete_parentheses",
        "kind": 3,
        "detail": "def complete_parentheses() -> None",
        "sortText": "0",
        "insertText": "complete_parentheses($0)",
        "insertTextFormat": 2
      }
    ]
    "#);

    Ok(())
}

#[test]
fn complete_function_parentheses_without_snippet_support() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def complete_parentheses() -> None: ...

complete_parenth
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_complete_function_parentheses(true),
        )
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(2, 16));
    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "complete_parentheses",
        "kind": 3,
        "detail": "def complete_parentheses() -> None",
        "sortText": "0",
        "insertText": "complete_parentheses()"
      }
    ]
    "#);

    Ok(())
}

#[test]
fn complete_function_parentheses_preserves_qualified_label() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import typing

is_typedd
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_complete_function_parentheses(true),
        )
        .enable_completion_snippets(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let completions = server.completion_request(&server.file_uri(foo), Position::new(2, 8));
    insta::assert_json_snapshot!(completions, @r#"
    [
      {
        "label": "typing.is_typeddict",
        "kind": 3,
        "sortText": "0",
        "insertText": "typing.is_typeddict($0)",
        "insertTextFormat": 2
      }
    ]
    "#);

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

#[test]
fn documentation_prefers_markdown_when_listed_first() -> Result<()> {
    assert_eq!(
        documentation_format(vec![MarkupKind::Markdown, MarkupKind::PlainText])?,
        MarkupKind::Markdown,
    );
    Ok(())
}

#[test]
fn documentation_prefers_plain_text_when_listed_first() -> Result<()> {
    assert_eq!(
        documentation_format(vec![MarkupKind::PlainText, MarkupKind::Markdown])?,
        MarkupKind::PlainText,
    );
    Ok(())
}

#[test]
fn documentation_supports_only_markdown() -> Result<()> {
    assert_eq!(
        documentation_format(vec![MarkupKind::Markdown])?,
        MarkupKind::Markdown
    );
    Ok(())
}

#[test]
fn documentation_supports_only_plain_text() -> Result<()> {
    assert_eq!(
        documentation_format(vec![MarkupKind::PlainText])?,
        MarkupKind::PlainText
    );
    Ok(())
}

fn documentation_format(formats: Vec<MarkupKind>) -> Result<MarkupKind> {
    let workspace_root = SystemPath::new("src");
    let document_path = SystemPath::new("src/foo.py");
    let document_content = r#"def foo_with_documentation() -> None:
    """
    Example doc comment
    """
    ...

foo_
"#;
    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(document_path, document_content)?
        .with_completion_documentation_format(formats)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(document_path, document_content, 1);

    let completions =
        server.completion_request(&server.file_uri(document_path), Position::new(6, 4));

    let completion = completions
        .into_iter()
        .find(|completion| completion.label == "foo_with_documentation")
        .expect("Completion of function should exist");
    let documentation = completion
        .documentation
        .expect("Expected documentation in completion");

    let Documentation::MarkupContent(markup) = documentation else {
        panic!("Expected markup documentation");
    };

    Ok(markup.kind)
}
