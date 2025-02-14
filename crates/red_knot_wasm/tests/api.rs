#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use red_knot_wasm::{PythonVersion, Settings, Workspace};

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

    assert_eq!(
        result,
        vec![
            "\
error: lint:unresolved-import
 --> /test.py:1:8
  |
1 | import random22
  |        ^^^^^^^^ Cannot resolve import `random22`
  |
",
        ],
    );
}
