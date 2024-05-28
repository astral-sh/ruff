const PANDAS_HTML_SRC: &str = include_str!("../resources/test/fixtures/pandas_html.py");

use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
use ruff_server::{PositionEncoding, TextDocument};

#[test]
fn delete_lines_pandas_html() {
    let mut document = TextDocument::new(PANDAS_HTML_SRC.to_string(), 1);

    let changes = vec![
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 79,
                    character: 0,
                },
                end: Position {
                    line: 91,
                    character: 67,
                },
            }),
            range_length: Some(388),
            text: String::new(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 81,
                    character: 4,
                },
                end: Position {
                    line: 81,
                    character: 36,
                },
            }),
            range_length: Some(32),
            text: "p".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 81,
                    character: 5,
                },
                end: Position {
                    line: 81,
                    character: 5,
                },
            }),
            range_length: Some(0),
            text: "a".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 81,
                    character: 6,
                },
                end: Position {
                    line: 81,
                    character: 6,
                },
            }),
            range_length: Some(0),
            text: "s".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 81,
                    character: 7,
                },
                end: Position {
                    line: 81,
                    character: 7,
                },
            }),
            range_length: Some(0),
            text: "s".into(),
        },
    ];

    let mut version = 2;

    for change in changes {
        document.apply_changes(vec![change], version, PositionEncoding::UTF16);
        version += 1;
    }

    insta::assert_snapshot!(document.contents());
}
