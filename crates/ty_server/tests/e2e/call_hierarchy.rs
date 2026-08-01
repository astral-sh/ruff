use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    PartialResultParams, Position, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};
use lsp_types::{
    CallHierarchyIncomingCallsRequest, CallHierarchyOutgoingCallsRequest,
    CallHierarchyPrepareRequest,
};

use crate::TestServerBuilder;

#[test]
fn prepare_function() -> anyhow::Result<()> {
    let content = r#"def my_function():
    pass

result = my_function()
"#;

    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document("foo.py", content, 1);

    // Cursor on the `def my_function` name.
    let items = prepare(&mut server, "foo.py", Position::new(0, 6)).unwrap();
    insta::assert_json_snapshot!(items, @r#"
    [
      {
        "name": "my_function",
        "kind": 12,
        "detail": "foo",
        "uri": "file://<temp_dir>/foo.py",
        "range": {
          "start": {
            "line": 0,
            "character": 0
          },
          "end": {
            "line": 1,
            "character": 8
          }
        },
        "selectionRange": {
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

    Ok(())
}

#[test]
fn outgoing_calls() -> anyhow::Result<()> {
    let content = r#"def helper():
    pass

def caller():
    helper()
    helper()
"#;

    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document("foo.py", content, 1);

    // Position on `caller`.
    let items = prepare(&mut server, "foo.py", Position::new(3, 4)).unwrap();
    let calls = outgoing(&mut server, items[0].clone()).unwrap();
    insta::assert_json_snapshot!(calls, @r#"
    [
      {
        "to": {
          "name": "helper",
          "kind": 12,
          "detail": "foo",
          "uri": "file://<temp_dir>/foo.py",
          "range": {
            "start": {
              "line": 0,
              "character": 0
            },
            "end": {
              "line": 1,
              "character": 8
            }
          },
          "selectionRange": {
            "start": {
              "line": 0,
              "character": 4
            },
            "end": {
              "line": 0,
              "character": 10
            }
          }
        },
        "fromRanges": [
          {
            "start": {
              "line": 4,
              "character": 4
            },
            "end": {
              "line": 4,
              "character": 10
            }
          },
          {
            "start": {
              "line": 5,
              "character": 4
            },
            "end": {
              "line": 5,
              "character": 10
            }
          }
        ]
      }
    ]
    "#);

    Ok(())
}

#[test]
fn incoming_calls_multi_file() -> anyhow::Result<()> {
    let lib = r#"def func():
    pass
"#;
    let caller_a = r#"from lib import func

def use_a():
    func()
"#;
    let caller_b = r#"from lib import func

def use_b():
    func()
    func()
"#;

    let mut server = TestServerBuilder::new()?
        .with_file("lib.py", lib)?
        .with_file("caller_a.py", caller_a)?
        .with_file("caller_b.py", caller_b)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document("lib.py", lib, 1);

    let items = prepare(&mut server, "lib.py", Position::new(0, 4)).unwrap();
    let mut calls = incoming(&mut server, items[0].clone()).unwrap();
    // Sort by caller name so the snapshot does not depend on discovery order.
    calls.sort_by(|a, b| a.from.name.cmp(&b.from.name));
    // In particular, this records that `fromRanges` use the caller files'
    // line indexes rather than the prepared definition's file.
    insta::assert_json_snapshot!(calls, @r#"
    [
      {
        "from": {
          "name": "use_a",
          "kind": 12,
          "detail": "caller_a",
          "uri": "file://<temp_dir>/caller_a.py",
          "range": {
            "start": {
              "line": 2,
              "character": 0
            },
            "end": {
              "line": 3,
              "character": 10
            }
          },
          "selectionRange": {
            "start": {
              "line": 2,
              "character": 4
            },
            "end": {
              "line": 2,
              "character": 9
            }
          }
        },
        "fromRanges": [
          {
            "start": {
              "line": 3,
              "character": 4
            },
            "end": {
              "line": 3,
              "character": 8
            }
          }
        ]
      },
      {
        "from": {
          "name": "use_b",
          "kind": 12,
          "detail": "caller_b",
          "uri": "file://<temp_dir>/caller_b.py",
          "range": {
            "start": {
              "line": 2,
              "character": 0
            },
            "end": {
              "line": 4,
              "character": 10
            }
          },
          "selectionRange": {
            "start": {
              "line": 2,
              "character": 4
            },
            "end": {
              "line": 2,
              "character": 9
            }
          }
        },
        "fromRanges": [
          {
            "start": {
              "line": 3,
              "character": 4
            },
            "end": {
              "line": 3,
              "character": 8
            }
          },
          {
            "start": {
              "line": 4,
              "character": 4
            },
            "end": {
              "line": 4,
              "character": 8
            }
          }
        ]
      }
    ]
    "#);

    Ok(())
}

fn prepare(
    server: &mut crate::TestServer,
    path: impl AsRef<ruff_db::system::SystemPath>,
    position: Position,
) -> Option<Vec<CallHierarchyItem>> {
    server.send_request_await::<CallHierarchyPrepareRequest>(CallHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: server.file_uri(path),
            },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    })
}

fn incoming(
    server: &mut crate::TestServer,
    item: CallHierarchyItem,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    server.send_request_await::<CallHierarchyIncomingCallsRequest>(
        CallHierarchyIncomingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
}

fn outgoing(
    server: &mut crate::TestServer,
    item: CallHierarchyItem,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    server.send_request_await::<CallHierarchyOutgoingCallsRequest>(
        CallHierarchyOutgoingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
}
