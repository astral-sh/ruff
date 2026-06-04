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
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    assert_eq!(
        sub_diagnostics
            .iter()
            .map(|sub_diagnostic| (
                sub_diagnostic.severity.as_str(),
                sub_diagnostic.message.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                "info",
                "Searched in the following paths during module resolution:"
            ),
            ("info", "  1. / (first-party code)"),
            (
                "info",
                "  2. vendored://stdlib (stdlib typeshed stubs vendored by ty)"
            ),
            (
                "info",
                "make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment"
            ),
        ]
    );
    assert!(
        sub_diagnostics
            .iter()
            .all(|sub_diagnostic| sub_diagnostic.location.is_none())
    );
}

#[wasm_bindgen_test]
fn annotated_sub_diagnostics_have_ranges() {
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
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    let function_detail = sub_diagnostics
        .iter()
        .find(|sub_diagnostic| sub_diagnostic.message == "Function defined here")
        .expect("Expected a function definition sub-diagnostic");
    let function_location = function_detail
        .location
        .as_ref()
        .expect("Expected a function definition location");

    assert_eq!(function_detail.severity, "info");
    assert_eq!(function_location.path, "/test.py");
    assert_eq!(
        function_location.range.start,
        Position { line: 3, column: 5 }
    );
}
