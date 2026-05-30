#![cfg(target_arch = "wasm32")]

use ty_wasm::{Position, PositionEncoding, Workspace};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn check() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file("test.py", "import random22\n")
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");

    assert_eq!(result.len(), 1);

    let diagnostic = &result[0];

    assert_eq!(diagnostic.id(), "unresolved-import");
    assert_eq!(
        diagnostic.to_range(&workspace).unwrap().start,
        Position { line: 1, column: 8 }
    );
    assert_eq!(
        diagnostic.message(),
        "Cannot resolve imported module `random22`"
    );
    let details = diagnostic.details(&workspace);
    assert_eq!(
        details
            .iter()
            .map(|detail| detail.message.as_str())
            .collect::<Vec<_>>(),
        [
            "info: Searched in the following paths during module resolution:",
            "info:   1. / (first-party code)",
            "info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)",
            "info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment",
        ]
    );
    assert!(details.iter().all(|detail| detail.range.is_none()));
}

#[wasm_bindgen_test]
fn annotated_sub_diagnostic_details_have_ranges() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file(
            "test.py",
            "from collections.abc import Buffer\n\n\
def f(x: Buffer | list[str] | int): ...\n\n\
f(x=\"foo\")\n",
        )
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");
    let diagnostic = &result[0];
    let details = diagnostic.details(&workspace);
    let function_detail = details
        .iter()
        .find(|detail| detail.message == "info: Function defined here")
        .expect("Expected a function definition sub-diagnostic");

    assert_eq!(function_detail.path.as_deref(), Some("/test.py"));
    assert_eq!(
        function_detail.range.unwrap().start,
        Position { line: 3, column: 5 }
    );
}
