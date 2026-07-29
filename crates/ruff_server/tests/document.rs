const PANDAS_HTML_SRC: &str = include_str!("../resources/test/fixtures/pandas_html.py");

use lsp_types::{
    Position, Range, TextDocumentContentChangeEvent, TextDocumentContentChangePartial,
};
use ruff_server::{PositionEncoding, TextDocument};

#[test]
fn delete_lines_pandas_html() {
    let mut document = TextDocument::new(PANDAS_HTML_SRC.to_string(), 1);

    let changes = vec![
        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
            TextDocumentContentChangePartial {
                range: Range {
                    start: Position {
                        line: 79,
                        character: 0,
                    },
                    end: Position {
                        line: 91,
                        character: 67,
                    },
                },
                text: String::new(),
                ..Default::default()
            },
        ),
        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
            TextDocumentContentChangePartial {
                range: Range {
                    start: Position {
                        line: 81,
                        character: 4,
                    },
                    end: Position {
                        line: 81,
                        character: 36,
                    },
                },
                text: "p".into(),
                ..Default::default()
            },
        ),
        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
            TextDocumentContentChangePartial {
                range: Range {
                    start: Position {
                        line: 81,
                        character: 5,
                    },
                    end: Position {
                        line: 81,
                        character: 5,
                    },
                },
                text: "a".into(),
                ..Default::default()
            },
        ),
        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
            TextDocumentContentChangePartial {
                range: Range {
                    start: Position {
                        line: 81,
                        character: 6,
                    },
                    end: Position {
                        line: 81,
                        character: 6,
                    },
                },
                text: "s".into(),
                ..Default::default()
            },
        ),
        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
            TextDocumentContentChangePartial {
                range: Range {
                    start: Position {
                        line: 81,
                        character: 7,
                    },
                    end: Position {
                        line: 81,
                        character: 7,
                    },
                },
                text: "s".into(),
                ..Default::default()
            },
        ),
    ];

    for (version, change) in (2..).zip(changes) {
        document.apply_changes(vec![change], version, PositionEncoding::UTF16);
    }

    insta::assert_snapshot!(document.contents());
}
