use lsp_types::request::{
    CallHierarchyIncomingCalls, CallHierarchyOutgoingCalls, CallHierarchyPrepare,
};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    PartialResultParams, Position, Range, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
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
    assert_eq!(items.len(), 1, "expected one item, got {items:?}");
    assert_eq!(items[0].name, "my_function");
    assert_eq!(items[0].kind, lsp_types::SymbolKind::FUNCTION);

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
    assert_eq!(items[0].name, "caller");

    let calls = outgoing(&mut server, items[0].clone()).unwrap();
    // One callee group (`helper`), two call sites.
    assert_eq!(calls.len(), 1, "got {calls:?}");
    assert_eq!(calls[0].to.name, "helper");
    assert_eq!(calls[0].from_ranges.len(), 2);

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
    assert_eq!(items[0].name, "func");

    let mut calls = incoming(&mut server, items[0].clone()).unwrap();
    // Sort by caller name for stable assertions.
    calls.sort_by(|a, b| a.from.name.cmp(&b.from.name));
    let names: Vec<_> = calls.iter().map(|c| c.from.name.as_str()).collect();
    assert!(
        names.contains(&"use_a") && names.contains(&"use_b"),
        "got callers: {names:?}"
    );

    // Assert exact call-site ranges (not just count). `caller_b.py` has
    // `    func()` at lines 3 and 4 — both `func` identifiers span columns 4..8.
    // Crucially this verifies `from_ranges` are converted using the *caller's*
    // line index, not the prepared file's (a regression we shipped once: the
    // wrong file index produces absurd column numbers on cross-file callers).
    let use_b = calls.iter().find(|c| c.from.name == "use_b").unwrap();
    let mut b_ranges = use_b.from_ranges.clone();
    b_ranges.sort_by_key(|r| (r.start.line, r.start.character));
    assert_eq!(
        b_ranges,
        vec![
            Range::new(Position::new(3, 4), Position::new(3, 8)),
            Range::new(Position::new(4, 4), Position::new(4, 8)),
        ],
        "use_b call-site ranges mismatch"
    );

    let use_a = calls.iter().find(|c| c.from.name == "use_a").unwrap();
    assert_eq!(
        use_a.from_ranges,
        vec![Range::new(Position::new(3, 4), Position::new(3, 8))],
        "use_a call-site range mismatch"
    );

    Ok(())
}

fn prepare(
    server: &mut crate::TestServer,
    path: impl AsRef<ruff_db::system::SystemPath>,
    position: Position,
) -> Option<Vec<CallHierarchyItem>> {
    server.send_request_await::<CallHierarchyPrepare>(CallHierarchyPrepareParams {
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
    server.send_request_await::<CallHierarchyIncomingCalls>(CallHierarchyIncomingCallsParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    })
}

fn outgoing(
    server: &mut crate::TestServer,
    item: CallHierarchyItem,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    server.send_request_await::<CallHierarchyOutgoingCalls>(CallHierarchyOutgoingCallsParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    })
}
