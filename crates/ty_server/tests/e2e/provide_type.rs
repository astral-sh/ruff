use crate::TestServerBuilder;
use anyhow::Context;
use lsp_types::{Position, Range};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;
use ty_server::server::api::requests::provide_type::ProvideTypeResponse;

fn assert_provide_type_snapshot(
    file_content: &str,
    request_range: Range,
) -> anyhow::Result<ProvideTypeResponse> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, Some(ClientOptions::default()))?
        .with_file(foo, file_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, file_content, 1);
    server
        .provide_type_request(foo, request_range)
        .context("Unable to request type")
}

#[test]
fn provide_str_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
class C:
    pass
def foo() -> C:
    return C()
",
        Range::new(Position::new(3, 11), Position::new(3, 14)),
    )?;
    insta::assert_json_snapshot!("provide_str_type", &provide_type_response);
    Ok(())
}

#[test]
fn provide_nested_class_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
class A:
    class B:
        pass

b = A.B()
b
",
        Range::new(Position::new(5, 0), Position::new(5, 1)),
    )?;
    insta::assert_json_snapshot!("provide_nested_class_type", &provide_type_response);
    Ok(())
}

#[test]
fn provide_class_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
class A:
    pass
A
",
        Range::new(Position::new(2, 0), Position::new(2, 1)),
    )?;
    insta::assert_json_snapshot!("provide_class_type", &provide_type_response);
    Ok(())
}
