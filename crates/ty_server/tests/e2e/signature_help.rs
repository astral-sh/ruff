use anyhow::{Context, Result};
use lsp_types::{
    ActiveParameter, ClientCapabilities, ClientSignatureInformationOptions, Position,
    SignatureHelpClientCapabilities, TextDocumentClientCapabilities,
};
use ruff_db::system::SystemPath;
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
fn active_parameter_capabilities() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import overload

def f(x: int, y: int): pass

f(x=1, 2)
f(1, 2)

@overload
def g(a: int, *, b: int): ...
@overload
def g(a: int, *, c: int): ...
def g(a: int, **kwargs: int): ...

g(1, b=2)
";

    for (active_parameter_support, no_active_parameter_support) in [
        (false, None),
        (true, None),
        (false, Some(false)),
        (true, Some(false)),
        (false, Some(true)),
        (true, Some(true)),
    ] {
        let capabilities = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                signature_help: Some(SignatureHelpClientCapabilities {
                    signature_information: Some(ClientSignatureInformationOptions {
                        active_parameter_support: Some(active_parameter_support),
                        no_active_parameter_support,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut server = TestServerBuilder::new()?
            .with_client_capabilities(capabilities)
            .enable_workspace_configuration(true)
            .enable_pull_diagnostics(true)
            .with_initialization_options(&ClientOptions::default())
            .with_workspace(workspace_root, None)?
            .with_file(foo, foo_content)?
            .build()
            .wait_until_workspaces_are_initialized();
        server.open_text_document(foo, foo_content, 1);

        let unmatched_parameter = no_active_parameter_support
            .and_then(|supported| supported.then_some(ActiveParameter::Null));
        let matched_signature_parameter =
            active_parameter_support.then_some(ActiveParameter::Int(1));
        let unmatched_signature_parameter = if active_parameter_support {
            unmatched_parameter
        } else {
            None
        };

        let mut active_parameters = |position| -> Result<_> {
            let signature_help = server
                .signature_help_request(&server.file_uri(foo), position)
                .with_context(|| format!("Missing signature help at {position:?}"))?;
            Ok((
                signature_help.active_parameter,
                signature_help
                    .signatures
                    .iter()
                    .map(|signature| signature.active_parameter)
                    .collect::<Vec<_>>(),
            ))
        };

        assert_eq!(
            active_parameters(Position::new(4, 8))?,
            (unmatched_parameter, vec![unmatched_signature_parameter]),
            "f(x=1, 2), activeParameterSupport={active_parameter_support}, noActiveParameterSupport={no_active_parameter_support:?}",
        );
        assert_eq!(
            active_parameters(Position::new(5, 6))?,
            (
                Some(ActiveParameter::Int(1)),
                vec![matched_signature_parameter]
            ),
            "f(1, 2), activeParameterSupport={active_parameter_support}, noActiveParameterSupport={no_active_parameter_support:?}",
        );
        assert_eq!(
            active_parameters(Position::new(13, 8))?,
            (
                Some(ActiveParameter::Int(1)),
                vec![matched_signature_parameter, unmatched_signature_parameter],
            ),
            "g(1, b=2), activeParameterSupport={active_parameter_support}, noActiveParameterSupport={no_active_parameter_support:?}",
        );
    }

    Ok(())
}
