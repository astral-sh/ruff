#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use ruff_linter::registry::Rule;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_wasm::{ExpandedMessage, Workspace};

macro_rules! check {
    ($source:expr, $config:expr, $expected:expr) => {{
        let config = js_sys::JSON::parse($config).unwrap();
        match Workspace::new(config).unwrap().check($source) {
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
            code: Some(Rule::IfTuple.noqa_code().to_string()),
            message: "If test is a tuple, which is always `True`".to_string(),
            location: SourceLocation {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(3)
            },
            end_location: SourceLocation {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(9)
            },
            fix: None,
        }]
    );
}

#[wasm_bindgen_test]
fn syntax_error() {
    check!(
        "x =\ny = 1\n",
        r#"{}"#,
        [ExpandedMessage {
            code: None,
            message: "SyntaxError: Expected an expression".to_string(),
            location: SourceLocation {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(3)
            },
            end_location: SourceLocation {
                row: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            },
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
