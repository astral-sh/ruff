#![cfg(target_arch = "wasm32")]

use js_sys;
use rustpython_parser::ast::Location;
use wasm_bindgen_test::*;

use ruff::registry::Rule;
use ruff_wasm::*;

macro_rules! check {
    ($source:expr, $config:expr, $expected:expr) => {{
        let foo = js_sys::JSON::parse($config).unwrap();
        match check($source, foo) {
            Ok(output) => {
                let result: Vec<ExpandedMessage> = serde_wasm_bindgen::from_value(output).unwrap();
                assert_eq!(result, $expected);
            }
            Err(e) => assert!(false, "{:#?}", e),
        }
    }};
}

#[wasm_bindgen_test]
fn empty_config() {
    check!(
        "if (1, 2):\n    pass",
        r#"{}"#,
        [ExpandedMessage {
            code: Rule::IfTuple.noqa_code().to_string(),
            message: "If test is a tuple, which is always `True`".to_string(),
            location: Location::new(1, 0),
            end_location: Location::new(2, 8),
            fix: None,
        }]
    );
}

#[wasm_bindgen_test]
fn partial_config() {
    check!("if (1, 2):\n    pass", r#"{"ignore": ["F"]}"#, []);
}

#[wasm_bindgen_test]
fn partial_nested_config() {
    let config = r#"{
          "select": ["Q"],
          "flake8-quotes": {
            "inline-quotes": "single"
          }
        }"#;
    check!(r#"print('hello world')"#, config, []);
}
