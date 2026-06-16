#![cfg(target_arch = "wasm32")]

use ty_wasm::{Position, PositionEncoding, SubDiagnosticSeverity, Workspace};
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
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    assert_eq!(
        sub_diagnostics
            .iter()
            .map(|sub_diagnostic| (sub_diagnostic.severity, sub_diagnostic.message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                SubDiagnosticSeverity::Info,
                "Searched in the following paths during module resolution:"
            ),
            (SubDiagnosticSeverity::Info, "  1. / (first-party code)"),
            (
                SubDiagnosticSeverity::Info,
                "  2. vendored://stdlib (stdlib typeshed stubs vendored by ty)"
            ),
            (
                SubDiagnosticSeverity::Info,
                "make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment"
            ),
        ]
    );
    assert!(
        sub_diagnostics
            .iter()
            .all(|sub_diagnostic| sub_diagnostic.annotations.is_empty())
    );
}

#[wasm_bindgen_test]
fn annotated_sub_diagnostics_include_all_annotations() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file(
            "test.py",
            "from collections.abc import Buffer\n\n\
def f(x: Buffer | list[str] | int): ...\n\n\
f(x=\"foo\")\n",
        )
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");
    let diagnostic = &result[0];
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    let function_detail = sub_diagnostics
        .iter()
        .find(|sub_diagnostic| sub_diagnostic.message == "Function defined here")
        .expect("Expected a function definition sub-diagnostic");

    assert_eq!(function_detail.severity, SubDiagnosticSeverity::Info);
    assert_eq!(function_detail.annotations.len(), 2);

    let function_annotation = &function_detail.annotations[0];
    assert!(function_annotation.primary);
    assert_eq!(function_annotation.message, None);
    let function_location = function_annotation
        .location
        .as_ref()
        .expect("Expected a function definition location");
    assert_eq!(function_location.path, "/test.py");
    assert_eq!(
        function_location.range.start,
        Position { line: 3, column: 5 }
    );

    let parameter_annotation = &function_detail.annotations[1];
    assert!(!parameter_annotation.primary);
    assert_eq!(
        parameter_annotation.message.as_deref(),
        Some("Parameter declared here")
    );
    assert_eq!(
        parameter_annotation
            .location
            .as_ref()
            .expect("Expected a parameter location")
            .path,
        "/test.py"
    );
}

#[wasm_bindgen_test]
fn sub_diagnostics_include_multiple_primary_annotations() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file(
            "test.py",
            "\
class Eggs: ...

class VeryEggyOmelette(
    Eggs,
    Eggs,
    Eggs
): ...
",
        )
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");
    let diagnostic = result
        .iter()
        .find(|diagnostic| diagnostic.id() == "duplicate-base")
        .expect("Expected a duplicate-base diagnostic");
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    let definition_detail = sub_diagnostics
        .iter()
        .find(|sub_diagnostic| {
            sub_diagnostic.message
                == "The definition of class `VeryEggyOmelette` will raise `TypeError` at runtime"
        })
        .expect("Expected a class definition sub-diagnostic");

    assert_eq!(definition_detail.annotations.len(), 3);
    assert_eq!(
        definition_detail
            .annotations
            .iter()
            .map(|annotation| annotation.primary)
            .collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(
        definition_detail
            .annotations
            .iter()
            .map(|annotation| annotation.message.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("Class `Eggs` first included in bases list here"),
            Some("Class `Eggs` later repeated here"),
            Some("Class `Eggs` later repeated here"),
        ]
    );
}

#[wasm_bindgen_test]
fn primary_diagnostic_annotations_preserve_order() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file("test.py", "value: int = \"foo\"\n")
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");
    let diagnostic = result
        .iter()
        .find(|diagnostic| diagnostic.id() == "invalid-assignment")
        .expect("Expected an invalid-assignment diagnostic");
    let annotations = diagnostic.annotations(&workspace);

    assert_eq!(
        annotations
            .iter()
            .map(|annotation| annotation.primary)
            .collect::<Vec<_>>(),
        [true, false]
    );
    let annotation = &annotations[1];
    assert_eq!(annotation.message.as_deref(), Some("Declared type"));

    let location = annotation
        .location
        .as_ref()
        .expect("Expected a declared type location");
    assert_eq!(location.path, "/test.py");
    assert_eq!(location.range.start, Position { line: 1, column: 8 });
}

#[wasm_bindgen_test]
fn secondary_only_sub_diagnostic_annotations_have_messages_and_locations() {
    ty_wasm::before_main();

    let mut workspace = Workspace::new(
        "/",
        PositionEncoding::Utf32,
        js_sys::JSON::parse("{}").unwrap(),
    )
    .expect("Workspace to be created");

    workspace
        .open_file(
            "test.py",
            r"\
from typing_extensions import TypedDict

class Movie(TypedDict):
    name: str

movie: Movie = {'name': 'Blade Runner'}
del movie['name']
",
        )
        .expect("File to be opened");

    let result = workspace.check().expect("Check to succeed");
    let diagnostic = result
        .iter()
        .find(|diagnostic| {
            diagnostic.message().as_string().as_deref()
                == Some("Cannot delete required key \"name\" from TypedDict `Movie`")
        })
        .unwrap_or_else(|| {
            panic!(
                "Expected an invalid TypedDict deletion diagnostic, got: {:?}",
                result
                    .iter()
                    .map(|diagnostic| diagnostic.message().as_string())
                    .collect::<Vec<_>>()
            )
        });
    let sub_diagnostics = diagnostic.sub_diagnostics(&workspace);
    let field_detail = sub_diagnostics
        .iter()
        .find(|sub_diagnostic| sub_diagnostic.message == "Field defined here")
        .expect("Expected a field definition sub-diagnostic");

    assert_eq!(field_detail.severity, SubDiagnosticSeverity::Info);
    assert_eq!(field_detail.annotations.len(), 3);
    assert!(
        field_detail
            .annotations
            .iter()
            .all(|annotation| !annotation.primary)
    );
    assert_eq!(
        field_detail
            .annotations
            .iter()
            .map(|annotation| annotation.message.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("`name` declared as required here"),
            Some("Consider making it `NotRequired`"),
            Some("`Movie` defined here"),
        ]
    );
    assert!(
        field_detail
            .annotations
            .iter()
            .all(|annotation| annotation.location.is_some())
    );
    assert_eq!(
        field_detail.annotations[0]
            .location
            .as_ref()
            .expect("Expected a field declaration location")
            .range
            .start,
        Position { line: 5, column: 5 }
    );
    assert_eq!(
        field_detail.annotations[2]
            .location
            .as_ref()
            .expect("Expected a class definition location")
            .range
            .start,
        Position { line: 4, column: 7 }
    );
}
