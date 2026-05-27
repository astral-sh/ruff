use anyhow::Result;
use lsp_types::request::{GotoImplementation, GotoImplementationParams};
use lsp_types::{
    ClientCapabilities, DiagnosticClientCapabilities, GotoCapability, GotoDefinitionResponse,
    ImplementationProviderCapability, PartialResultParams, Position,
    PublishDiagnosticsClientCapabilities, Range, TextDocumentClientCapabilities,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    WorkspaceClientCapabilities,
};

use crate::TestServerBuilder;

const CONTENT: &str = r#"class Animal:
    def speak(self): ...

class Dog(Animal):
    def speak(self): ...

class Cat(Animal):
    def speak(self): ...

def f(animal: Animal):
    animal.speak()
"#;

#[test]
fn implementation_provider_is_advertised() -> Result<()> {
    let server = TestServerBuilder::new()?
        .build()
        .wait_until_workspaces_are_initialized();

    let initialization_result = server.initialization_result().unwrap();
    assert_eq!(
        initialization_result.capabilities.implementation_provider,
        Some(ImplementationProviderCapability::Simple(true))
    );

    Ok(())
}

#[test]
fn implementation_locations_without_link_support() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", CONTENT)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("foo.py", CONTENT, 1);

    let response = implementation(&mut server, "foo.py", Position::new(10, 13)).unwrap();
    let GotoDefinitionResponse::Array(locations) = response else {
        panic!("Expected Location[] response, got {response:#?}");
    };

    let ranges: Vec<_> = locations.iter().map(|location| location.range).collect();
    assert_eq!(
        ranges,
        vec![
            Range::new(Position::new(1, 8), Position::new(1, 13)),
            Range::new(Position::new(4, 8), Position::new(4, 13)),
            Range::new(Position::new(7, 8), Position::new(7, 13)),
        ]
    );

    Ok(())
}

#[test]
fn implementation_location_links_with_link_support() -> Result<()> {
    let capabilities = capabilities_with_implementation_link_support();
    let mut server = TestServerBuilder::new()?
        .with_file("foo.py", CONTENT)?
        .with_client_capabilities(capabilities)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("foo.py", CONTENT, 1);

    let response = implementation(&mut server, "foo.py", Position::new(10, 13)).unwrap();
    let GotoDefinitionResponse::Link(links) = response else {
        panic!("Expected LocationLink[] response, got {response:#?}");
    };

    let selection_ranges: Vec<_> = links
        .iter()
        .map(|link| link.target_selection_range)
        .collect();
    assert_eq!(
        selection_ranges,
        vec![
            Range::new(Position::new(1, 8), Position::new(1, 13)),
            Range::new(Position::new(4, 8), Position::new(4, 13)),
            Range::new(Position::new(7, 8), Position::new(7, 13)),
        ]
    );
    assert!(links.iter().all(|link| {
        link.origin_selection_range
            == Some(Range::new(Position::new(10, 11), Position::new(10, 16)))
    }));

    Ok(())
}

fn implementation(
    server: &mut crate::TestServer,
    path: impl AsRef<ruff_db::system::SystemPath>,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    server.send_request_await::<GotoImplementation>(GotoImplementationParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: server.file_uri(path),
            },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    })
}

fn capabilities_with_implementation_link_support() -> ClientCapabilities {
    ClientCapabilities {
        text_document: Some(TextDocumentClientCapabilities {
            implementation: Some(GotoCapability {
                link_support: Some(true),
                ..GotoCapability::default()
            }),
            diagnostic: Some(DiagnosticClientCapabilities::default()),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
            ..TextDocumentClientCapabilities::default()
        }),
        workspace: Some(WorkspaceClientCapabilities {
            configuration: Some(true),
            ..WorkspaceClientCapabilities::default()
        }),
        ..ClientCapabilities::default()
    }
}
