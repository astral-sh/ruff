use anyhow::Result;
use lsp_types::{LspRequestMethod, MessageDirection, Request};
use ruff_db::system::SystemPath;
use serde_json::{Value, json};

use crate::TestServerBuilder;

enum DiscoverTests {}

impl Request for DiscoverTests {
    type Params = Value;
    type Result = Value;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/discoverTests");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

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
    let test_example_content = "\
def test_dummy():
    pass
";
    let test_other = SystemPath::new("src/tests/test_other.py");
    let test_other_content = "\
class Test:
    def method_test():
        pass
";
    let main = SystemPath::new("src/main.py");
    let main_content = "\
def test_should_not_be_found():
    pass


def main():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_example, test_example_content)?
        .with_file(test_other, test_other_content)?
        .with_file(main, main_content)?
        .enable_pull_diagnostics(false)
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
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_across_multiple_workspaces() -> Result<()> {
    let workspace_one = SystemPath::new("workspace_one");
    let workspace_two = SystemPath::new("workspace_two");
    // Both workspaces have a file with the same name, showing that ids don't
    // collide, since they embed each workspace's own absolute path.
    let test_one = SystemPath::new("workspace_one/test_example.py");
    let test_one_content = "\
def test_first():
    pass
";
    let test_two = SystemPath::new("workspace_two/test_example.py");
    let test_two_content = "\
def test_second():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_one, None)?
        .with_file(test_one, test_one_content)?
        .with_workspace(workspace_two, None)?
        .with_file(test_two, test_two_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

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
          "id": "<temp_dir>/workspace_one/test_example.py",
          "kind": "file",
          "label": "test_example.py",
          "parent": "<temp_dir>/workspace_one",
          "uri": "file://<temp_dir>/workspace_one/test_example.py"
        },
        {
          "id": "<temp_dir>/workspace_one/test_example.py::test_first",
          "kind": "function",
          "label": "test_first",
          "parent": "<temp_dir>/workspace_one/test_example.py",
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
          "uri": "file://<temp_dir>/workspace_one/test_example.py"
        },
        {
          "id": "<temp_dir>/workspace_two",
          "kind": "directory",
          "label": "workspace_two",
          "uri": "file://<temp_dir>/workspace_two/"
        },
        {
          "id": "<temp_dir>/workspace_two/test_example.py",
          "kind": "file",
          "label": "test_example.py",
          "parent": "<temp_dir>/workspace_two",
          "uri": "file://<temp_dir>/workspace_two/test_example.py"
        },
        {
          "id": "<temp_dir>/workspace_two/test_example.py::test_second",
          "kind": "function",
          "label": "test_second",
          "parent": "<temp_dir>/workspace_two/test_example.py",
          "range": {
            "end": {
              "character": 15,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_two/test_example.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_single_file() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let module_1 = SystemPath::new("src/tests/test_module_1.py");
    let module_1_content = "\
def test_one():
    pass
";
    let module_2 = SystemPath::new("src/tests/test_module_2.py");
    let module_2_content = "\
def test_two():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(module_1, module_1_content)?
        .with_file(module_2, module_2_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    // The client sends a full `file://` uri to the module it wants tests for, e.g.
    // `file:///Users/.../hue-control/tests/test_enable_alarm.py`.
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
          "id": "<temp_dir>/src/tests/test_module_1.py::test_one",
          "kind": "function",
          "label": "test_one",
          "parent": "<temp_dir>/src/tests/test_module_1.py",
          "range": {
            "end": {
              "character": 12,
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
fn discover_tests_scoped_to_a_module_with_same_relative_path_in_another_workspace() -> Result<()> {
    let workspace_one = SystemPath::new("workspace_one");
    let workspace_two = SystemPath::new("workspace_two");

    // Both workspaces have a `tests/test_module_1.py` and a `tests/test_module_2.py` at
    // the same relative path, so a request scoped to workspace_one's module_1 must not
    // pick up workspace_two's identically-pathed module_1, or either workspace's module_2.
    let module_1_one = SystemPath::new("workspace_one/tests/test_module_1.py");
    let module_1_one_content = "\
def test_one_alpha():
    pass
";
    let module_2_one = SystemPath::new("workspace_one/tests/test_module_2.py");
    let module_2_one_content = "\
def test_two_alpha():
    pass
";
    let module_1_two = SystemPath::new("workspace_two/tests/test_module_1.py");
    let module_1_two_content = "\
def test_one_beta():
    pass
";
    let module_2_two = SystemPath::new("workspace_two/tests/test_module_2.py");
    let module_2_two_content = "\
def test_two_beta():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_one, None)?
        .with_file(module_1_one, module_1_one_content)?
        .with_file(module_2_one, module_2_one_content)?
        .with_workspace(workspace_two, None)?
        .with_file(module_1_two, module_1_two_content)?
        .with_file(module_2_two, module_2_two_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let uri = server.file_uri(module_1_one);
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
          "id": "<temp_dir>/workspace_one/tests/test_module_1.py",
          "kind": "file",
          "label": "test_module_1.py",
          "parent": "<temp_dir>/workspace_one/tests",
          "uri": "file://<temp_dir>/workspace_one/tests/test_module_1.py"
        },
        {
          "id": "<temp_dir>/workspace_one/tests/test_module_1.py::test_one_alpha",
          "kind": "function",
          "label": "test_one_alpha",
          "parent": "<temp_dir>/workspace_one/tests/test_module_1.py",
          "range": {
            "end": {
              "character": 18,
              "line": 0
            },
            "start": {
              "character": 4,
              "line": 0
            }
          },
          "uri": "file://<temp_dir>/workspace_one/tests/test_module_1.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_single_directory() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let unit_directory = SystemPath::new("src/tests/unit");
    let unit_module = SystemPath::new("src/tests/unit/test_module.py");
    let unit_module_content = "\
def test_unit():
    pass
";
    let integration_module = SystemPath::new("src/tests/integration/test_module.py");
    let integration_module_content = "\
def test_integration():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(unit_module, unit_module_content)?
        .with_file(integration_module, integration_module_content)?
        .enable_pull_diagnostics(false)
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
          "id": "<temp_dir>/src/tests/unit/test_module.py::test_unit",
          "kind": "function",
          "label": "test_unit",
          "parent": "<temp_dir>/src/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 13,
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

#[test]
fn discover_tests_scoped_to_a_directory_with_same_relative_path_in_another_workspace() -> Result<()>
{
    let workspace_one = SystemPath::new("workspace_one");
    let workspace_two = SystemPath::new("workspace_two");

    // Both workspaces have a `tests/unit` and a `tests/integration` directory at the same
    // relative path, so a request scoped to workspace_one's `tests/unit` must not pick up
    // workspace_two's identically-pathed directory, or either workspace's `tests/integration`.
    let unit_directory_one = SystemPath::new("workspace_one/tests/unit");
    let unit_module_one = SystemPath::new("workspace_one/tests/unit/test_module.py");
    let unit_module_one_content = "\
def test_unit_alpha():
    pass
";
    let integration_module_one = SystemPath::new("workspace_one/tests/integration/test_module.py");
    let integration_module_one_content = "\
def test_integration_alpha():
    pass
";
    let unit_module_two = SystemPath::new("workspace_two/tests/unit/test_module.py");
    let unit_module_two_content = "\
def test_unit_beta():
    pass
";
    let integration_module_two = SystemPath::new("workspace_two/tests/integration/test_module.py");
    let integration_module_two_content = "\
def test_integration_beta():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_one, None)?
        .with_file(unit_module_one, unit_module_one_content)?
        .with_file(integration_module_one, integration_module_one_content)?
        .with_workspace(workspace_two, None)?
        .with_file(unit_module_two, unit_module_two_content)?
        .with_file(integration_module_two, integration_module_two_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let uri = server.file_uri(unit_directory_one);
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
          "id": "<temp_dir>/workspace_one/tests/unit/test_module.py::test_unit_alpha",
          "kind": "function",
          "label": "test_unit_alpha",
          "parent": "<temp_dir>/workspace_one/tests/unit/test_module.py",
          "range": {
            "end": {
              "character": 19,
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
    let inside_content = "\
def test_inside():
    pass
";
    // This file exists on disk but no open workspace contains it.
    let outside = SystemPath::new("elsewhere/test_module.py");
    let outside_content = "\
def test_outside():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(inside, inside_content)?
        .with_file(outside, outside_content)?
        .enable_pull_diagnostics(false)
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
