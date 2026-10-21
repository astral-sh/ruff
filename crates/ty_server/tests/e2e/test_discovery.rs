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

/// Discovery follows pytest's rules instead of the project's settings, so it finds the tests in
/// files that are excluded from type checking. Requests for the whole project, the directory and
/// the file all report the same tests.
#[test]
fn discover_tests_that_are_excluded_from_type_checking() -> Result<()> {
    let excluded_directory = SystemPath::new("src/tests");
    let excluded_module = SystemPath::new("src/tests/test_module.py");

    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(
            SystemPath::new("src/ty.toml"),
            "[src]\nexclude = [\"tests\"]\n",
        )?
        .with_file(excluded_module, DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let all_tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    let uri = server.file_uri(excluded_directory);
    let directory_tests =
        sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
    assert_eq!(directory_tests, all_tests);

    let uri = server.file_uri(excluded_module);
    let module_tests =
        sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
    assert_eq!(module_tests, all_tests);

    insta::assert_json_snapshot!(all_tests, @r#"
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
          "id": "<temp_dir>/src/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/tests/test_module.py",
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
          "uri": "file://<temp_dir>/src/tests/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

/// Discovery doesn't descend into the directories that pytest skips by default: those matching
/// pytest's default `norecursedirs` patterns, `__pycache__`, and virtual environments, including
/// conda environments. A request for one of these directories, or for a path inside one, reports
/// nothing either.
#[test]
fn discover_tests_skips_directories_that_pytest_skips() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(SystemPath::new("src/env/pyvenv.cfg"), "")?
        .with_file(SystemPath::new("src/conda_env/conda-meta/history"), "")?
        .with_files(
            [
                "src/tests/test_module.py",
                "src/env/lib/test_module.py",
                "src/conda_env/lib/test_module.py",
                "src/.tox/py313/test_module.py",
                "src/node_modules/package/test_module.py",
                "src/build/test_module.py",
                "src/dist/test_module.py",
                "src/venv/test_module.py",
                "src/package.egg/test_module.py",
                "src/__pycache__/test_module.py",
                "src/CVS/test_module.py",
                "src/_darcs/test_module.py",
                "src/{arch}/test_module.py",
            ]
            .map(|path| (path, DUMMY_TEST)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    for path in ["src/.tox/py313/test_module.py", "src/.tox/py313", "src/env"] {
        let uri = server.file_uri(SystemPath::new(path));
        let tests = server.send_request_await::<DiscoverTests>(json!({ "uri": uri }));
        assert_eq!(tests, json!({ "tests": [] }), "request for `{path}`");
    }

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
          "id": "<temp_dir>/src/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/src/tests",
          "uri": "file://<temp_dir>/src/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/src/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/tests/test_module.py",
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
          "uri": "file://<temp_dir>/src/tests/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

/// The skip rules only apply below the directory that is searched, so a project with a name that
/// pytest skips is still searched. Requests for the project and for a file inside it report the
/// same tests.
#[test]
fn discover_tests_in_a_project_that_pytest_skips() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("build"), None)?
        .with_file(SystemPath::new("build/tests/test_module.py"), DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    insta::assert_json_snapshot!(tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/build",
          "kind": "directory",
          "label": "build",
          "uri": "file://<temp_dir>/build/"
        },
        {
          "id": "<temp_dir>/build/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/build",
          "uri": "file://<temp_dir>/build/tests/"
        },
        {
          "id": "<temp_dir>/build/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/build/tests",
          "uri": "file://<temp_dir>/build/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/build/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/build/tests/test_module.py",
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
          "uri": "file://<temp_dir>/build/tests/test_module.py"
        }
      ]
    }
    "#);

    for path in ["build", "build/tests/test_module.py"] {
        let uri = server.file_uri(SystemPath::new(path));
        let path_tests =
            sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
        assert_eq!(path_tests, tests, "request for `{path}`");
    }

    Ok(())
}

/// Like pytest, discovery doesn't read ignore files, so it finds the tests in ignored paths. The
/// test uses a `.ignore` file rather than a `.gitignore` file because the directory walker's
/// standard filters honor `.ignore` files even outside a git repository, so this test fails if
/// those filters are turned on.
#[test]
fn discover_tests_in_ignored_files() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(SystemPath::new("src/.ignore"), "generated/\n")?
        .with_file(SystemPath::new("src/generated/test_gen.py"), DUMMY_TEST)?
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
          "id": "<temp_dir>/src/generated",
          "kind": "directory",
          "label": "generated",
          "parent": "<temp_dir>/src",
          "uri": "file://<temp_dir>/src/generated/"
        },
        {
          "id": "<temp_dir>/src/generated/test_gen.py",
          "kind": "file",
          "label": "test_gen.py",
          "parent": "<temp_dir>/src/generated",
          "uri": "file://<temp_dir>/src/generated/test_gen.py"
        },
        {
          "id": "<temp_dir>/src/generated/test_gen.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/src/generated/test_gen.py",
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
          "uri": "file://<temp_dir>/src/generated/test_gen.py"
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

const OUTER_WORKSPACE: &str = "repo";
const INNER_DIRECTORY: &str = "repo/inner/tests";
const INNER_MODULE: &str = "repo/inner/tests/test_module.py";

fn server_with_nested_workspaces() -> Result<TestServer> {
    let server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new(OUTER_WORKSPACE), None)?
        .with_file(SystemPath::new("repo/tests/test_module.py"), DUMMY_TEST)?
        .with_workspace(SystemPath::new("repo/inner"), None)?
        .with_file(SystemPath::new(INNER_MODULE), DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    Ok(server)
}

#[test]
fn discover_tests_nested_workspaces_all() -> Result<()> {
    let mut server = server_with_nested_workspaces()?;

    let all_tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    let uri = server.file_uri(SystemPath::new(OUTER_WORKSPACE));
    let outer_tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
    assert_eq!(outer_tests, all_tests);

    insta::assert_json_snapshot!(all_tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/repo",
          "kind": "directory",
          "label": "repo",
          "uri": "file://<temp_dir>/repo/"
        },
        {
          "id": "<temp_dir>/repo/inner",
          "kind": "directory",
          "label": "inner",
          "uri": "file://<temp_dir>/repo/inner/"
        },
        {
          "id": "<temp_dir>/repo/inner/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/repo/inner",
          "uri": "file://<temp_dir>/repo/inner/tests/"
        },
        {
          "id": "<temp_dir>/repo/inner/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/repo/inner/tests",
          "uri": "file://<temp_dir>/repo/inner/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/repo/inner/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/repo/inner/tests/test_module.py",
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
          "uri": "file://<temp_dir>/repo/inner/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/repo/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/repo",
          "uri": "file://<temp_dir>/repo/tests/"
        },
        {
          "id": "<temp_dir>/repo/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/repo/tests",
          "uri": "file://<temp_dir>/repo/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/repo/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/repo/tests/test_module.py",
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
          "uri": "file://<temp_dir>/repo/tests/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_nested_workspaces_inner() -> Result<()> {
    let mut server = server_with_nested_workspaces()?;

    let uri = server.file_uri(SystemPath::new(INNER_MODULE));
    let module_tests =
        sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));

    let uri = server.file_uri(SystemPath::new(INNER_DIRECTORY));
    let directory_tests =
        sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
    assert_eq!(directory_tests, module_tests);

    insta::assert_json_snapshot!(module_tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/repo/inner",
          "kind": "directory",
          "label": "inner",
          "uri": "file://<temp_dir>/repo/inner/"
        },
        {
          "id": "<temp_dir>/repo/inner/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/repo/inner",
          "uri": "file://<temp_dir>/repo/inner/tests/"
        },
        {
          "id": "<temp_dir>/repo/inner/tests/test_module.py",
          "kind": "file",
          "label": "test_module.py",
          "parent": "<temp_dir>/repo/inner/tests",
          "uri": "file://<temp_dir>/repo/inner/tests/test_module.py"
        },
        {
          "id": "<temp_dir>/repo/inner/tests/test_module.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/repo/inner/tests/test_module.py",
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
          "uri": "file://<temp_dir>/repo/inner/tests/test_module.py"
        }
      ]
    }
    "#);

    Ok(())
}

#[test]
fn discover_tests_with_the_project_root_above_the_workspace_folder() -> Result<()> {
    let workspace_root = SystemPath::new("b/src");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(SystemPath::new("b/ty.toml"), "")?
        .with_file(SystemPath::new("b/src/tests/test_a.py"), DUMMY_TEST)?
        .with_file(SystemPath::new("b/tests/test_b.py"), DUMMY_TEST)?
        .build()
        .wait_until_workspaces_are_initialized();

    let all_tests = sort_by_id(server.send_request_await::<DiscoverTests>(json!({})));

    let uri = server.file_uri(workspace_root);
    let workspace_tests =
        sort_by_id(server.send_request_await::<DiscoverTests>(json!({ "uri": uri })));
    assert_eq!(workspace_tests, all_tests);

    insta::assert_json_snapshot!(all_tests, @r#"
    {
      "tests": [
        {
          "id": "<temp_dir>/b/src",
          "kind": "directory",
          "label": "src",
          "uri": "file://<temp_dir>/b/src/"
        },
        {
          "id": "<temp_dir>/b/src/tests",
          "kind": "directory",
          "label": "tests",
          "parent": "<temp_dir>/b/src",
          "uri": "file://<temp_dir>/b/src/tests/"
        },
        {
          "id": "<temp_dir>/b/src/tests/test_a.py",
          "kind": "file",
          "label": "test_a.py",
          "parent": "<temp_dir>/b/src/tests",
          "uri": "file://<temp_dir>/b/src/tests/test_a.py"
        },
        {
          "id": "<temp_dir>/b/src/tests/test_a.py::test_dummy",
          "kind": "function",
          "label": "test_dummy",
          "parent": "<temp_dir>/b/src/tests/test_a.py",
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
          "uri": "file://<temp_dir>/b/src/tests/test_a.py"
        }
      ]
    }
    "#);

    let uri = server.file_uri(SystemPath::new("b/tests/test_b.py"));
    let outside_tests = server.send_request_await::<DiscoverTests>(json!({ "uri": uri }));
    assert_eq!(outside_tests, json!({ "tests": [] }));

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
