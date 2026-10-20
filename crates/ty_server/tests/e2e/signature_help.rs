use anyhow::Result;
use lsp_types::{Documentation, MarkupContent, MarkupKind, Position, SignatureInformation};
use ruff_db::system::SystemPath;
use ruff_python_trivia::textwrap::dedent;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

/// Tests that we get signature help even when the cursor
/// is on the function name.
///
/// This is a regression test to ensure we don't accidentally
/// cause this case to stop working.
#[test]
fn works_in_function_name() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import re
re.match('', '')
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(&ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let signature_help = server.signature_help_request(&server.file_uri(foo), Position::new(1, 6));

    insta::assert_json_snapshot!(signature_help);

    Ok(())
}

#[test]
fn documentation_uses_plain_text_when_advertised() -> Result<()> {
    let source = dedent(
        r#"
        def greet(name: str):
            """Greet a person.

            Args:
                name: The person's name.
            """
            ...

        greet(
        "#,
    );
    let builder = TestServerBuilder::new()?
        .with_signature_documentation_format(Some(vec![MarkupKind::PlainText]));

    let signature = signature_information(&source, Position::new(9, 6), builder)?;

    assert_eq!(
        signature.documentation,
        Some(Documentation::String(
            "Greet a person.\n\nArgs:\n    name: The person's name.\n".to_string()
        ))
    );
    let parameter = &signature.parameters.as_ref().expect("parameters")[0];
    assert_eq!(
        parameter.documentation,
        Some(Documentation::String("The person's name.".to_string()))
    );
    Ok(())
}

#[test]
fn documentation_uses_markdown_when_advertised() -> Result<()> {
    let source = dedent(
        r#"
        def greet(name: str):
            """<p>Greet a person.</p>

            Args:
                name: A <code>name</code>.
            """
            ...

        greet(
        "#,
    );
    let builder = TestServerBuilder::new()?
        .with_signature_documentation_format(Some(vec![MarkupKind::Markdown]))
        .with_allowed_html_tags(&["p", "code"]);

    let signature = signature_information(&source, Position::new(9, 6), builder)?;

    let Some(Documentation::MarkupContent(documentation)) = &signature.documentation else {
        panic!("Expected Markdown signature documentation");
    };
    assert_eq!(documentation.kind, MarkupKind::Markdown);
    assert!(documentation.value.contains("<p>Greet a person.</p>"));
    assert!(documentation.value.contains("A <code>name</code>."));
    let parameter = &signature.parameters.as_ref().expect("parameters")[0];
    assert_eq!(
        parameter.documentation,
        Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "A <code>name</code>.".to_string(),
        }))
    );
    Ok(())
}

fn signature_information(
    source: &str,
    position: Position,
    builder: TestServerBuilder,
) -> Result<SignatureInformation> {
    let workspace_root = SystemPath::new("src");
    let path = SystemPath::new("src/foo.py");
    let mut server = builder
        .with_workspace(workspace_root, None)?
        .with_file(path, source)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document(path, source, 1);

    let help = server
        .signature_help_request(&server.file_uri(path), position)
        .expect("signature help");
    Ok(help.signatures.into_iter().next().expect("signature"))
}
