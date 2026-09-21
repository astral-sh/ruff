use anyhow::Result;
use lsp_types::{LspRequestMethod, MessageDirection, Request};
use ruff_db::system::SystemPath;
use serde_json::{Value, json};

use crate::{TestServer, TestServerBuilder};

enum DiscoverTests {}

impl Request for DiscoverTests {
    type Params = Value;
    type Result = Value;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/discoverTests");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

const DUMMY_TEST: &str = "\
def test_dummy():
    pass
";

const NON_TEST: &str = "\
def test_should_not_be_found():
    pass
";

fn sort_by_id(mut response: Value) -> Value {
    if let Some(Value::Array(items)) = response.get_mut("tests") {
        items.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    response
}

#[test]
fn discover_all_tests() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_example = SystemPath::new("src/tests/test_example.py");
    let test_other = SystemPath::new("src/tests/test_other.py");
    let test_other_content = "\
class Test:
    def test_method(self):
        pass
";
    let main = SystemPath::new("src/main.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_example, DUMMY_TEST)?
        .with_file(test_other, test_other_content)?
        .with_file(main, NON_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/src",
          "kind": "directory",
          "label": "src",
          "uri": "file://<temp_dir>/src/"
        },
        {
          "id": "<temp_dir>/src/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/src",
          "uri": "file://<temp_dir>/src/tests/"
        },
        {
          "id": "<temp_dir>/src/tests/test_example.py",
          "kind": "file",
          "label": "test_example.py",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/test_example.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_example.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/tests/test_example.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/src/tests/test_example.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_other.py",
          "kind": "file",
          "label": "test_other.py",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/test_other.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_other.py::Test",
          "kind": "class",
          "label": "Test",
          "parent": "<temp_dir>/src/tests/test_other.py",
          "range": {
            "end": {
              "character": 10,
              "line": 0
            },
            "start": {
              "character": 6,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/src/tests/test_other.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_other.py::Test::test_method",
          "kind": "function",
          "label": "test_method",
          "parent": "<temp_dir>/src/tests/test_other.py::Test",
          "range": {
            "end": {
              "character": 19,
              "line": 1
            },
            "start": {
              "character": 8,
              "line": 1
            }
          },
          "uri": "file://<temp_dir>/src/tests/test_other.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_file() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let module_1 = SystemPath::new("src/tests/test_module_1.py");
    let module_2 = SystemPath::new("src/tests/test_module_2.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(module_1, DUMMY_TEST)?
        .with_file(module_2, DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let uri = server.file_uri(module_1);
    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/src",
          "kind": "directory",
          "label": "src",
          "uri": "file://<temp_dir>/src/"
        },
        {
          "id": "<temp_dir>/src/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/src",
          "uri": "file://<temp_dir>/src/tests/"
        },
        {
          "id": "<temp_dir>/src/tests/test_module_1.py",
          "kind": "file",
          "label": "test_module_1.py",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/test_module_1.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_module_1.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/tests/test_module_1.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/src/tests/test_module_1.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_directory() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let unit_directory = SystemPath::new("src/tests/unit");
    let unit_module = SystemPath::new("src/tests/unit/test_module.py");
    let integration_module = SystemPath::new("src/tests/integration/test_module.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(unit_module, DUMMY_TEST)?
        .with_file(integration_module, DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    // A client can scope a request to a directory instead of a module, which asks for
    // every test underneath it and nothing from its sibling directories.
    let uri = server.file_uri(unit_directory);
    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/src",
          "kind": "directory",
          "label": "src",
          "uri": "file://<temp_dir>/src/"
        },
        {
          "id": "<temp_dir>/src/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/src",
          "uri": "file://<temp_dir>/src/tests/"
        },
        {
          "id": "<temp_dir>/src/tests/unit",
          "kind": "directory",
          "label": "unit",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/unit/"
        },
        {
          "id": "<temp_dir>/src/tests/unit/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/src/tests/unit",
          "uri": "file://<temp_dir>/src/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/src/tests/unit/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/src/tests/unit/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

const UNIT_DIRECTORY: &str = "workspace_one/tests/unit";
const UNIT_MODULE: &str = "workspace_one/tests/unit/test_module.py";
const INTEGRATION_MODULE: &str = "workspace_one/tests/integration/test_module.py";

/// Builds two workspaces with the same internal layout, so that an item resolved against the
/// wrong workspace shows up in a snapshot as an extra entry.
fn server_with_two_workspaces() -> Result<TestServer> {
    let server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("workspace_one"), None)?
        .with_file(SystemPath::new(UNIT_MODULE), DUMMY_TEST)?
        .with_file(SystemPath::new(INTEGRATION_MODULE), DUMMY_TEST)?
        .with_workspace(SystemPath::new("workspace_two"), None)?
        .with_file(
            SystemPath::new("workspace_two/tests/unit/test_module.py"),
            DUMMY_TEST,
        )?
        .with_file(
            SystemPath::new("workspace_two/tests/integration/test_module.py"),
            DUMMY_TEST,
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    Ok(server)
}

/// A request without a uri covers every workspace. Ids embed each workspace's own absolute
/// path, so the identically named modules stay distinct.
#[test]
fn discover_tests_two_workspaces_all() -> Result<()> {
    let mut server = server_with_two_workspaces()?;

    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/workspace_one",
          "kind": "directory",
          "label": "workspace_one",
          "uri": "file://<temp_dir>/workspace_one/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/workspace_one",
          "uri": "file://<temp_dir>/workspace_one/tests/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/integration",
          "kind": "directory",
          "label": "integration",
          "parent": "<temp_dir>/workspace_one/tests",
          "uri": "file://<temp_dir>/workspace_one/tests/integration/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/integration/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_one/tests/integration",
          "uri": "file://<temp_dir>/workspace_one/tests/integration/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/integration/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_one/tests/integration/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_one/tests/integration/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit",
          "kind": "directory",
          "label": "unit",
          "parent": "<temp_dir>/workspace_one/tests",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_one/tests/unit",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_two",
          "kind": "directory",
          "label": "workspace_two",
          "uri": "file://<temp_dir>/workspace_two/"
        },
        {
          "id": "<temp_dir>/workspace_two/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/workspace_two",
          "uri": "file://<temp_dir>/workspace_two/tests/"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/integration",
          "kind": "directory",
          "label": "integration",
          "parent": "<temp_dir>/workspace_two/tests",
          "uri": "file://<temp_dir>/workspace_two/tests/integration/"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/integration/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_two/tests/integration",
          "uri": "file://<temp_dir>/workspace_two/tests/integration/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/integration/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_two/tests/integration/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_two/tests/integration/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/unit",
          "kind": "directory",
          "label": "unit",
          "parent": "<temp_dir>/workspace_two/tests",
          "uri": "file://<temp_dir>/workspace_two/tests/unit/"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/unit/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_two/tests/unit",
          "uri": "file://<temp_dir>/workspace_two/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_two/tests/unit/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_two/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_two/tests/unit/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

/// Scoping to a module excludes the module of the same name in the other workspace, and the
/// sibling module in this one.
#[test]
fn discover_tests_two_workspaces_one_file() -> Result<()> {
    let mut server = server_with_two_workspaces()?;

    let uri = server.file_uri(SystemPath::new(UNIT_MODULE));
    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/workspace_one",
          "kind": "directory",
          "label": "workspace_one",
          "uri": "file://<temp_dir>/workspace_one/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/workspace_one",
          "uri": "file://<temp_dir>/workspace_one/tests/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit",
          "kind": "directory",
          "label": "unit",
          "parent": "<temp_dir>/workspace_one/tests",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_one/tests/unit",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

/// Scoping to a directory excludes the directory of the same name in the other workspace, and
/// the sibling directory in this one.
#[test]
fn discover_tests_two_workspaces_one_dir() -> Result<()> {
    let mut server = server_with_two_workspaces()?;

    let uri = server.file_uri(SystemPath::new(UNIT_DIRECTORY));
    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/workspace_one",
          "kind": "directory",
          "label": "workspace_one",
          "uri": "file://<temp_dir>/workspace_one/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/workspace_one",
          "uri": "file://<temp_dir>/workspace_one/tests/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit",
          "kind": "directory",
          "label": "unit",
          "parent": "<temp_dir>/workspace_one/tests",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/workspace_one/tests/unit",
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 14,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_one/tests/unit/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_for_a_uri_outside_any_workspace() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let inside = SystemPath::new("src/tests/test_module.py");
    let outside = SystemPath::new("elsewhere/test_module.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(inside, DUMMY_TEST)?
        .with_file(outside, DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let uri = server.file_uri(outside);
    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": []
    }
    "#);

    Ok(())
}
