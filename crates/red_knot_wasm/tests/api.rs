#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use red_knot_wasm::{Settings, TargetVersion, Workspace};

fn setup_workspace() -> Workspace {
    Workspace::new(
        "/",
        Settings {
            target_version: TargetVersion::Py312,
        },
    )
    .expect("Workspace to be created")
}

#[wasm_bindgen_test]
fn check() {
    let mut workspace = setup_workspace();
    let test = workspace
        .open_file("test.py", "import random22\n")
        .expect("File to be opened");

    let result = workspace.check_file(test).expect("Check to succeed");

    assert_eq!(result, vec!["Unresolved import 'random22'"]);
}
