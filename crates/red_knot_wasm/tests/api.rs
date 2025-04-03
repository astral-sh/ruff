#![cfg(target_arch = "wasm32")]

use red_knot_wasm::{Position, Workspace};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn check() {
    let mut workspace =
        Workspace::new("/", js_sys::JSON::parse("{}").unwrap()).expect("Workspace to be created");

    workspace
        .open_file("test.py", "import random22\n")
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");

    assert_eq!(result.len(), 1);

    let diagnostic = &result[0];

    assert_eq!(diagnostic.id(), "lint:unresolved-import");
    assert_eq!(
        diagnostic.to_range(&workspace).unwrap().start,
        Position { line: 1, column: 8 }
    );
    assert_eq!(diagnostic.message(), "Cannot resolve import `random22`");
}
