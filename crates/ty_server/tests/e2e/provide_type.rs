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
fn provide_int_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
a = int(10)
a
",
        Range::new(Position::new(1, 0), Position::new(1, 1)),
    )?;
    insta::assert_json_snapshot!("provide_int_type", &provide_type_response);
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
fn provide_generic_class_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
class A[T]:
    i: T
    def __init__(self, i: T):
        self.i = i

class B:
    pass

a = A(B())
a
",
        Range::new(Position::new(9, 0), Position::new(9, 1)),
    )?;
    insta::assert_json_snapshot!("provide_generic_class_type", &provide_type_response);
    Ok(())
}

#[test]
fn provide_integer_literal_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
a = 1
a
",
        Range::new(Position::new(1, 0), Position::new(1, 1)),
    )?;
    insta::assert_json_snapshot!("provide_integer_literal_type", &provide_type_response);
    Ok(())
}

#[test]
fn provide_callable_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
def a() -> int:
    return 1
a()
",
        Range::new(Position::new(2, 0), Position::new(2, 1)),
    )?;
    insta::assert_json_snapshot!("provide_callable_type", &provide_type_response);
    Ok(())
}

#[test]
fn provide_function_with_default_parameter_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
def a(b, c=1) -> int:
    return 1
a()
",
        Range::new(Position::new(2, 0), Position::new(2, 1)),
    )?;
    insta::assert_json_snapshot!(
        "provide_function_with_default_parameter_type",
        &provide_type_response
    );
    Ok(())
}

#[test]
fn provide_class_local_to_function_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
def a():
    class A:
        pass
    a = A()
    a
",
        Range::new(Position::new(4, 4), Position::new(4, 5)),
    )?;
    insta::assert_json_snapshot!(
        "provide_class_local_to_function_type",
        &provide_type_response
    );
    Ok(())
}

#[test]
fn provide_type_variable_type() -> anyhow::Result<()> {
    let provide_type_response = assert_provide_type_snapshot(
        "\
class A[T1]:
    def f[T2](self, t: T1 | T2):
        t
",
        Range::new(Position::new(2, 8), Position::new(2, 9)),
    )?;
    insta::assert_json_snapshot!("provide_type_variable_type", &provide_type_response);
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
