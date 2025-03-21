#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use red_knot_wasm::{Position, PythonVersion, Settings, Workspace};

#[wasm_bindgen_test]
fn check() {
    let settings = Settings {
        python_version: PythonVersion::Py312,
    };
    let mut workspace = Workspace::new("/", &settings).expect("Workspace to be created");

    workspace
        .open_file("test.py", "import random22\n")
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");

    assert_eq!(result.len(), 1);

    let diagnostic = &result[0];

    assert_eq!(diagnostic.id(), "lint:unresolved-import");
    assert_eq!(
        diagnostic.to_range(&workspace).unwrap().start,
        Position {
            line: 0,
            character: 7
        }
    );
    assert_eq!(diagnostic.message(), "Cannot resolve import `random22`");
}
