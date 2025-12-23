use crate::TestServerBuilder;
use crate::notebook::NotebookBuilder;
use insta::assert_json_snapshot;

#[test]
fn text_document() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", "")?
        .enable_pull_diagnostics(true)
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

#[test]
fn notebook() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("test.ipynb", "")?
        .enable_pull_diagnostics(true)
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
