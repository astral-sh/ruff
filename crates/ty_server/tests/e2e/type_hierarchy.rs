use lsp_types::request::{TypeHierarchyPrepare, TypeHierarchySubtypes, TypeHierarchySupertypes};
use lsp_types::{
    PartialResultParams, Position, TextDocumentIdentifier, TextDocumentPositionParams,
    TypeHierarchyPrepareParams, TypeHierarchySubtypesParams, TypeHierarchySupertypesParams,
    WorkDoneProgressParams,
};

use crate::TestServerBuilder;

#[test]
fn simple_supertypes() -> anyhow::Result<()> {
    let content = r#"class Base:
    pass

class Derived(Base):
    pass
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("foo.py", content, 1);

    // Prepare on `Derived`
    let items = prepare(&mut server, "foo.py", Position::new(3, 8)).unwrap();
    assert_eq!(items[0].name, "Derived");

    // Get supertypes of `Derived`
    let bases = supertypes(&mut server, items[0].clone()).unwrap();
    assert_eq!(bases.len(), 1);
    assert_eq!(bases[0].name, "Base");

    Ok(())
}

/// Tests that we can query for multiple subtypes.
#[test]
fn simple_subtypes() -> anyhow::Result<()> {
    let content = r#"class Base:
    pass

class Child1(Base):
    pass

class Child2(Base):
    pass
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("foo.py", content, 1);

    let items = prepare(&mut server, "foo.py", Position::new(0, 8)).unwrap();
    assert_eq!(items[0].name, "Base");

    let children = subtypes(&mut server, items[0].clone()).unwrap();
    assert_eq!(children.len(), 2);

    let names: Vec<_> = children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Child1"));
    assert!(names.contains(&"Child2"));

    Ok(())
}

/// This tests that we can start at a class and then issue
/// repeated supertype requests until we reach the top of
/// the class hierarchy (`object`).
#[test]
fn chained_hierarchy() -> anyhow::Result<()> {
    let content = r#"class Grandparent:
    pass

class Parent(Grandparent):
    pass

class Child(Parent):
    pass
"#;

    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document("foo.py", content, 1);

    // Start at Child, and walk up the class hierarchy.
    let items = prepare(&mut server, "foo.py", Position::new(6, 8)).unwrap();
    assert_eq!(items[0].name, "Child");

    let parents = supertypes(&mut server, items[0].clone()).unwrap();
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].name, "Parent");

    let grandparents = supertypes(&mut server, parents[0].clone()).unwrap();
    assert_eq!(grandparents.len(), 1);
    assert_eq!(grandparents[0].name, "Grandparent");

    let top = supertypes(&mut server, grandparents[0].clone()).unwrap();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].name, "object");

    // `object` has no supertypes
    let beyond = supertypes(&mut server, top[0].clone());
    assert!(beyond.is_none());

    Ok(())
}

/// Tests that the type hierarchy works for types defined in vendored
/// (typeshed) files, where the document URI provided by the client
/// points to a cached system path that must be mapped back to a
/// vendored path.
///
/// This is a regression test that the initial type hierarchy
/// implementation failed. In particular, the system path to a
/// vendored file provided by the client wasn't being mapped back to
/// a `VendoredPath`, and this in turn ultimately resulted in two
/// different interned `File` values for the same typeshed file. This
/// led to downstream issues related to type equality.
#[test]
fn vendored_supertypes() -> anyhow::Result<()> {
    let content = "from enum import StrEnum";
    let mut server = TestServerBuilder::new()?
        .enable_pull_diagnostics(true)
        .with_file("foo.py", content)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document("foo.py", content, 1);

    let items = prepare(&mut server, "foo.py", Position::new(0, 20)).unwrap();
    assert_eq!(items[0].name, "StrEnum");

    // Note that we don't actually assert anything about the
    // URI in `items[0]`. This test matches the actual flow
    // that failed, which is the actually important thing to
    // test.
    let bases = supertypes(&mut server, items[0].clone()).unwrap();
    let names: Vec<_> = bases.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"str"));
    assert!(names.contains(&"ReprEnum"));

    // Follow `ReprEnum` to its supertypes — another vendored round-trip
    // that exercises resolving the vendored URI back to a `File`.
    let repr_enum = bases.iter().find(|b| b.name == "ReprEnum").unwrap();
    let repr_enum_bases = supertypes(&mut server, repr_enum.clone()).unwrap();
    let names: Vec<_> = repr_enum_bases.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"Enum"));

    Ok(())
}

/// Sends a `textDocument/prepareTypeHierarchy` request.
fn prepare(
    server: &mut crate::TestServer,
    path: impl AsRef<ruff_db::system::SystemPath>,
    position: Position,
) -> Option<Vec<lsp_types::TypeHierarchyItem>> {
    server.send_request_await::<TypeHierarchyPrepare>(TypeHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: server.file_uri(path),
            },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    })
}

/// Sends a `typeHierarchy/supertypes` request.
fn supertypes(
    server: &mut crate::TestServer,
    item: lsp_types::TypeHierarchyItem,
) -> Option<Vec<lsp_types::TypeHierarchyItem>> {
    server.send_request_await::<TypeHierarchySupertypes>(TypeHierarchySupertypesParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    })
}

/// Sends a `typeHierarchy/subtypes` request.
fn subtypes(
    server: &mut crate::TestServer,
    item: lsp_types::TypeHierarchyItem,
) -> Option<Vec<lsp_types::TypeHierarchyItem>> {
    server.send_request_await::<TypeHierarchySubtypes>(TypeHierarchySubtypesParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    })
}
