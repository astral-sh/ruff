use crate::TestServerBuilder;
use crate::notebook::NotebookBuilder;
use anyhow::Context;
use insta::assert_json_snapshot;
use ruff_db::system::SystemPath;

#[test]
fn text_document() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", "")?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(
        "foo.py",
        r#"def test(): ...

test()
"#,
        1,
    );

    let edits = server
        .rename(
            &server.file_uri("foo.py"),
            lsp_types::Position {
                line: 0,
                character: 5,
            },
            "new_name",
        )
        .expect("Can rename `test` function");

    assert_json_snapshot!(edits);

    Ok(())
}

/// Standalone files outside the server's working directory support local renames.
#[test]
fn standalone_file_outside_working_directory() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = SystemPath::from_std_path(directory.path())
        .context("Temporary directory path must be UTF-8")?
        .join("standalone.py");
    let source = "a = 1\nb = a\nmissing\n";
    let mut server = TestServerBuilder::new()?
        .with_file(&path, source)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document(&path, source, 1);

    let diagnostics = server.document_diagnostic_request(&path, None);
    let lsp_types::DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) =
        diagnostics
    else {
        panic!("Expected a full diagnostic report");
    };
    let diagnostics = report.full_document_diagnostic_report.items;
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code,
        Some(lsp_types::Code::String("unresolved-reference".to_string()))
    );

    let edits = server
        .rename(
            &server.file_uri(&path),
            lsp_types::Position::new(0, 0),
            "renamed",
        )
        .expect("Can rename a standalone file's local variable")
        .context("Expected rename edits")?;
    let changes = edits.changes.context("Expected text edits")?;
    assert_eq!(changes.len(), 1);
    let edits = &changes[&server.file_uri(&path)];
    assert_eq!(edits.len(), 2);
    assert!(edits.iter().all(|edit| edit.new_text == "renamed"));
    assert_eq!(
        edits[0].range,
        lsp_types::Range::new(
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(0, 1)
        )
    );
    assert_eq!(
        edits[1].range,
        lsp_types::Range::new(
            lsp_types::Position::new(1, 4),
            lsp_types::Position::new(1, 5)
        )
    );

    Ok(())
}

#[test]
fn notebook() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("test.ipynb", "")?
        .build()
        .wait_until_workspaces_are_initialized();

    let mut builder = NotebookBuilder::virtual_file("test.ipynb");
    builder.add_python_cell(
        r#"from typing import Literal

type Style = Literal["italic", "bold", "underline"]"#,
    );

    let cell2 = builder.add_python_cell(
        r#"def with_style(line: str, word, style: Style) -> str:
    if style == "italic":
        return line.replace(word, f"*{word}*")
    elif style == "bold":
        return line.replace(word, f"__{word}__")

    position = line.find(word)
    output = line + "\n"
    output += " " * position
    output += "-" * len(word)
"#,
    );

    builder.open(&mut server);

    let edits = server
        .rename(
            &cell2,
            lsp_types::Position {
                line: 0,
                character: 16,
            },
            "text",
        )
        .expect("Can rename `line` parameter");

    assert_json_snapshot!(edits);

    server.collect_publish_diagnostic_notifications(2);
    Ok(())
}
