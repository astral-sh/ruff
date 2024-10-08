#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use red_knot_wasm::{Settings, TargetVersion, Workspace};

#[wasm_bindgen_test]
fn check() {
    let settings = Settings {
        target_version: TargetVersion::Py312,
    };
    let mut workspace = Workspace::new("/", &settings).expect("Workspace to be created");

    workspace
        .open_file("test.py", "import random22\n")
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");

    assert_eq!(
        result,
        vec!["/test.py:1:8: Cannot resolve import `random22`"]
    );
}
