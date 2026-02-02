#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

use ruff_linter::registry::Rule;
use ruff_source_file::OneIndexed;
use ruff_wasm::{ExpandedMessage, Location, PositionEncoding, Workspace};

macro_rules! check {
    ($source:expr, $config:expr, $expected:expr) => {{
        let config = js_sys::JSON::parse($config).unwrap();
        match Workspace::new(config, PositionEncoding::Utf8)
            .unwrap()
            .check($source)
        {
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
    ruff_wasm::before_main();

    check!(
        "if (1, 2):\n    pass",
        r#"{}"#,
        [ExpandedMessage {
            code: Rule::IfTuple.noqa_code().to_string(),
            message: "If test is a tuple, which is always `True`".to_string(),
            start_location: Location {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(3)
            },
            end_location: Location {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(9)
            },
            fix: None,
        }]
    );
}

#[wasm_bindgen_test]
fn syntax_error() {
    ruff_wasm::before_main();

    check!(
        "x =\ny = 1\n",
        r#"{}"#,
        [ExpandedMessage {
            code: "invalid-syntax".to_string(),
            message: "Expected an expression".to_string(),
            start_location: Location {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(3)
            },
            end_location: Location {
                row: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            },
            fix: None,
        }]
    );
}

#[wasm_bindgen_test]
fn unsupported_syntax_error() {
    ruff_wasm::before_main();

    check!(
        "match 2:\n    case 1: ...",
        r#"{"target-version": "py39"}"#,
        [ExpandedMessage {
            code: "invalid-syntax".to_string(),
            message: "Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)"
                .to_string(),
            start_location: Location {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(0)
            },
            end_location: Location {
                row: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(5)
            },
            fix: None,
        }]
    );
}

#[wasm_bindgen_test]
fn partial_config() {
    ruff_wasm::before_main();

    check!("if (1, 2):\n    pass", r#"{"ignore": ["F"]}"#, []);
}

#[wasm_bindgen_test]
fn partial_nested_config() {
    ruff_wasm::before_main();

    let config = r#"{
          "select": ["Q"],
          "flake8-quotes": {
            "inline-quotes": "single"
          }
        }"#;
    check!(r#"print('hello world')"#, config, []);
}
