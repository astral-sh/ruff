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
}
