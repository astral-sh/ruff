use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::Request as _;
use lsp_types::{DocumentFormattingRequest, DocumentRangeFormattingRequest, RegistrationRequest};

use crate::TestServerBuilder;

#[test]
fn statically_registers_formatting_when_dynamic_registration_is_unsupported() -> Result<()> {
    let server = TestServerBuilder::new()?.build();
    let capabilities = &server
        .initialization_result()
        .expect("Server should return initialization capabilities")
        .capabilities;

    assert_eq!(capabilities.document_formatting_provider, Some(true.into()));
    assert_eq!(
        capabilities.document_range_formatting_provider,
        Some(true.into())
    );

    Ok(())
}

#[test]
fn dynamically_registers_formatting_and_range_formatting_for_python_and_markdown() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .enable_formatting_dynamic_registration(true)
        .enable_range_formatting_dynamic_registration(true)
        .build();
    let capabilities = &server
        .initialization_result()
        .expect("Server should return initialization capabilities")
        .capabilities;

    assert_eq!(capabilities.document_formatting_provider, None);
    assert_eq!(capabilities.document_range_formatting_provider, None);

    let (_, params) = server.await_request::<RegistrationRequest>();
    let [formatting, range_formatting] = params.registrations.as_slice() else {
        panic!("Expected both dynamic formatting registrations");
    };

    assert_eq!(
        formatting.method,
        DocumentFormattingRequest::METHOD.as_str()
    );
    assert_eq!(
        range_formatting.method,
        DocumentRangeFormattingRequest::METHOD.as_str()
    );
    assert_json_snapshot!(
        formatting.register_options,
    @r#"
    {
      "documentSelector": [
        {
          "language": "python"
        },
        {
          "language": "markdown"
        },
        {
          "language": "python",
          "notebook": "*"
        }
      ]
    }
    "#
    );
    assert_eq!(
        range_formatting.register_options,
        formatting.register_options
    );

    Ok(())
}
