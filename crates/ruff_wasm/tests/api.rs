#![cfg(target_arch = "wasm32")]

use serde::Serialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

use ruff_linter::registry::Rule;
use ruff_source_file::OneIndexed;
use ruff_wasm::{
    ExpandedDiagnosticAnnotation, ExpandedDiagnosticLocation, ExpandedMessage,
    ExpandedSubDiagnostic, Location, PositionEncoding, SubDiagnosticSeverity, Workspace,
};

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

fn primary_annotation(
    start_location: Location,
    end_location: Location,
) -> ExpandedDiagnosticAnnotation {
    ExpandedDiagnosticAnnotation {
        primary: true,
        message: None,
        location: Some(ExpandedDiagnosticLocation {
            path: "<filename>".to_string(),
            start_location,
            end_location,
        }),
    }
}

fn property(value: &JsValue, name: &str) -> JsValue {
    js_sys::Reflect::get(value, &JsValue::from_str(name)).unwrap()
}

fn set_property(value: &JsValue, name: &str, property: &JsValue) {
    assert!(js_sys::Reflect::set(value, &JsValue::from_str(name), property).unwrap());
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
            annotations: vec![primary_annotation(
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(3),
                },
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(9),
                },
            )],
            sub_diagnostics: vec![],
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
            annotations: vec![primary_annotation(
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(3),
                },
                Location {
                    row: OneIndexed::from_zero_indexed(1),
                    column: OneIndexed::from_zero_indexed(0),
                },
            )],
            sub_diagnostics: vec![],
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
            annotations: vec![primary_annotation(
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(0),
                },
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(5),
                },
            )],
            sub_diagnostics: vec![],
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
fn sub_diagnostics() {
    ruff_wasm::before_main();

    let config = js_sys::JSON::parse(r#"{}"#).unwrap();
    let output = Workspace::new(config, PositionEncoding::Utf8)
        .unwrap()
        .check("import os\n")
        .unwrap();
    let result: Vec<ExpandedMessage> = serde_wasm_bindgen::from_value(output).unwrap();

    assert_eq!(result[0].message, "`os` imported but unused".to_string());
    assert_eq!(
        result[0].sub_diagnostics,
        [ExpandedSubDiagnostic {
            severity: SubDiagnosticSeverity::Help,
            message: "Remove unused import: `os`".to_string(),
            location: None,
        }]
    );
}

#[wasm_bindgen_test]
fn annotations_preserve_order() {
    ruff_wasm::before_main();

    let config = js_sys::JSON::parse(r#"{"select": ["B033"]}"#).unwrap();
    let output = Workspace::new(config, PositionEncoding::Utf8)
        .unwrap()
        .check("x = {1, 1}\n")
        .unwrap();
    let result: Vec<ExpandedMessage> = serde_wasm_bindgen::from_value(output).unwrap();

    assert_eq!(
        result[0].annotations,
        [
            primary_annotation(
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(8),
                },
                Location {
                    row: OneIndexed::from_zero_indexed(0),
                    column: OneIndexed::from_zero_indexed(9),
                },
            ),
            ExpandedDiagnosticAnnotation {
                primary: false,
                message: Some("Previous occurrence here".to_string()),
                location: Some(ExpandedDiagnosticLocation {
                    path: "<filename>".to_string(),
                    start_location: Location {
                        row: OneIndexed::from_zero_indexed(0),
                        column: OneIndexed::from_zero_indexed(5),
                    },
                    end_location: Location {
                        row: OneIndexed::from_zero_indexed(0),
                        column: OneIndexed::from_zero_indexed(6),
                    },
                }),
            }
        ]
    );
}

#[wasm_bindgen_test]
fn optional_fields_serialize_as_null() {
    ruff_wasm::before_main();

    let config = js_sys::JSON::parse(r#"{"select": ["F401"]}"#).unwrap();
    let workspace = Workspace::new(config, PositionEncoding::Utf8).unwrap();

    let output = workspace.check("x =\n").unwrap();
    let diagnostics = js_sys::Array::from(&output);
    assert!(property(&diagnostics.get(0), "fix").is_null());

    let output = workspace.check("import os\n").unwrap();
    let diagnostics = js_sys::Array::from(&output);
    let diagnostic = diagnostics.get(0);
    let fix = property(&diagnostic, "fix");
    let edits = js_sys::Array::from(&property(&fix, "edits"));
    assert!(property(&edits.get(0), "content").is_null());
    let sub_diagnostics = js_sys::Array::from(&property(&diagnostic, "subDiagnostics"));
    assert!(property(&sub_diagnostics.get(0), "location").is_null());
    let annotations = js_sys::Array::from(&property(&diagnostic, "annotations"));
    let annotation = annotations.get(0);
    assert!(property(&annotation, "message").is_null());

    // Ruff currently gives every fix a help message and every annotation a source location.
    // Round-trip explicit nulls to cover the nullable WASM contract for those fields anyway.
    set_property(&fix, "message", &JsValue::NULL);
    set_property(&annotation, "location", &JsValue::NULL);
    let messages: Vec<ExpandedMessage> = serde_wasm_bindgen::from_value(output).unwrap();
    let output = messages
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true))
        .unwrap();
    let diagnostics = js_sys::Array::from(&output);
    let diagnostic = diagnostics.get(0);
    let fix = property(&diagnostic, "fix");
    assert!(property(&fix, "message").is_null());
    let annotations = js_sys::Array::from(&property(&diagnostic, "annotations"));
    assert!(property(&annotations.get(0), "location").is_null());
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
