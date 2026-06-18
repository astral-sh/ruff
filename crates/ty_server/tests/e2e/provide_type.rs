use crate::TestServerBuilder;
use anyhow::Context;
use lsp_types::{Position, Range, TextDocumentIdentifier};
use ruff_db::system::SystemPath;
use ty_server::{ClientOptions, ProvideTypeParams, ProvideTypeRequest, ProvideTypeResponse};

#[test]
fn provide_type() -> anyhow::Result<()> {
    let workspace_root = SystemPath::new("src");
    let file = SystemPath::new("src/foo.py");
    let file_content = "\
class A:
    pass
import sys
A
sys
1.0
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, Some(ClientOptions::default()))?
        .with_file(file, file_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(file, file_content, 1);
    let uri = server.file_uri(file);
    let provide_type_response = server
        .send_request_await::<ProvideTypeRequest>(ProvideTypeParams {
            text_document: TextDocumentIdentifier { uri },
            ranges: vec![
                Range::new(Position::new(3, 0), Position::new(3, 1)),
                Range::new(Position::new(4, 0), Position::new(4, 3)),
                Range::new(Position::new(5, 0), Position::new(5, 3)),
            ],
        })
        .context("Unable to request type")?;
    assert_eq!(
        provide_type_response,
        ProvideTypeResponse {
            types: vec![
                Some("ty_extensions.TypeOf[foo.A]".to_string()),
                Some("Module[sys]".to_string()),
                Some("builtins.float".to_string()),
            ],
        }
    );
    Ok(())
}
