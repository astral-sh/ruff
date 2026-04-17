//! Tests the interaction of the `analyze graph` command.

#![cfg(not(target_arch = "wasm32"))]
#![cfg(not(windows))]

use assert_fs::prelude::*;
use std::process::Command;
use std::str;

use anyhow::Result;
use assert_fs::fixture::ChildPath;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use tempfile::TempDir;

fn command() -> Command {
    let mut command = Command::new(get_cargo_bin("ruff"));
    command.arg("analyze");
    command.arg("graph");
    command.arg("--preview");
    command.env_clear();
    command
}

const INSTA_FILTERS: &[(&str, &str)] = &[
    // Rewrite Windows output to Unix output
    (r"\\", "/"),
];

#[test]
fn dependencies() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        from ruff import c
    "#})?;
    root.child("ruff")
        .child("c.py")
        .write_str(indoc::indoc! {r#"
        from . import d
    "#})?;
    root.child("ruff")
        .child("d.py")
        .write_str(indoc::indoc! {r#"
        from .e import f
    "#})?;
    root.child("ruff")
        .child("e.py")
        .write_str(indoc::indoc! {r#"
        def f(): pass
    "#})?;
    root.child("ruff")
        .child("e.pyi")
        .write_str(indoc::indoc! {r#"
        def f() -> None: ...
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": [
            "ruff/d.py"
          ],
          "ruff/d.py": [
            "ruff/e.py",
            "ruff/e.pyi"
          ],
          "ruff/e.py": [],
          "ruff/e.pyi": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn dependents() -> Result<()> {
    let tempdir = TempDir::new()?;

    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        from ruff import c
    "#})?;
    root.child("ruff")
        .child("c.py")
        .write_str(indoc::indoc! {r#"
        from . import d
    "#})?;
    root.child("ruff")
        .child("d.py")
        .write_str(indoc::indoc! {r#"
        from .e import f
    "#})?;
    root.child("ruff")
        .child("e.py")
        .write_str(indoc::indoc! {r#"
        def f(): pass
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--direction").arg("dependents").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [],
          "ruff/b.py": [
            "ruff/a.py"
          ],
          "ruff/c.py": [
            "ruff/b.py"
          ],
          "ruff/d.py": [
            "ruff/c.py"
          ],
          "ruff/e.py": [
            "ruff/d.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn string_detection() -> Result<()> {
    let tempdir = TempDir::new()?;

    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        import importlib

        importlib.import_module("ruff.c")
    "#})?;
    root.child("ruff").child("c.py").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").arg("--min-dots").arg("1").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn string_detection_attribute() -> Result<()> {
    let tempdir = TempDir::new()?;

    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        import pydoc

        cls = pydoc.locate("ruff.c.MyClass")
    "#})?;
    root.child("ruff").child("c.py").write_str("")?;

    // Without string detection, no edge from b.py to c.py.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    // With string detection and min-dots=1, "ruff.c.MyClass" should resolve to ruff/c.py
    // by falling back from the unresolvable full name to the parent module.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").arg("--min-dots").arg("1").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    // With the default min-dots=2, the fallback from "ruff.c.MyClass" (2 dots)
    // would land on "ruff.c" (1 dot), which is below the threshold — so no edge.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// If the full string resolves as a module, no fallback should occur—the graph
/// should point at the exact file, not its parent.
#[test]
fn string_detection_exact_module() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        cls = "ruff.b"
    "#})?;
    root.child("ruff").child("b.py").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").arg("--min-dots").arg("1").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Multiple trailing components should be stripped until a module is found.
/// `"ruff.c.Outer.Inner"` should resolve to `ruff/c.py`.
#[test]
fn string_detection_deep_attribute() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        cls = "ruff.c.Outer.Inner"
    "#})?;
    root.child("ruff").child("c.py").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").arg("--min-dots").arg("1").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// With `min_dots=2` (the default), a string with 3+ dots can still fall back
/// as long as the resolved prefix retains at least 2 dots.
/// `"a.b.c.d.Cls"` (4 dots) → fallback to `"a.b.c.d"` (3 dots, ≥ 2) ✓.
#[test]
fn string_detection_min_dots_deep_fallback() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("a").child("__init__.py").write_str("")?;
    root.child("a")
        .child("b")
        .child("__init__.py")
        .write_str("")?;
    root.child("a")
        .child("b")
        .child("c")
        .child("__init__.py")
        .write_str("")?;
    root.child("a")
        .child("b")
        .child("c")
        .child("d.py")
        .write_str("")?;
    root.child("a")
        .child("b")
        .child("main.py")
        .write_str(indoc::indoc! {r#"
        cls = "a.b.c.d.MyClass"
    "#})?;

    // Default min_dots=2: "a.b.c.d" has 3 dots (≥ 2), so fallback succeeds.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "a/__init__.py": [],
          "a/b/__init__.py": [],
          "a/b/c/__init__.py": [],
          "a/b/c/d.py": [],
          "a/b/main.py": [
            "a/b/c/d.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// A string that passes the dot filter but where no prefix resolves to an
/// existing module should produce no edge.
#[test]
fn string_detection_unresolvable() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        cls = "nonexistent.module.Class"
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").arg("--min-dots").arg("1").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// String import fallback should work with `src` configuration, resolving
/// through non-root source directories.
#[test]
fn string_detection_with_src() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        src = ["lib"]

        [analyze]
        detect-string-imports = true
        string-imports-min-dots = 1
    "#})?;

    root.child("lib")
        .child("mylib")
        .child("__init__.py")
        .write_str("")?;
    root.child("lib")
        .child("mylib")
        .child("sub.py")
        .write_str("")?;

    root.child("app").child("__init__.py").write_str("")?;
    root.child("app")
        .child("main.py")
        .write_str(indoc::indoc! {r#"
        cls = "mylib.sub.MyClass"
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("app").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "app/__init__.py": [],
          "app/main.py": [
            "lib/mylib/sub.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn string_detection_from_config() -> Result<()> {
    let tempdir = TempDir::new()?;

    let root = ChildPath::new(tempdir.path());

    // Configure string import detection with a lower min-dots via ruff.toml
    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        [analyze]
        detect-string-imports = true
        string-imports-min-dots = 1
    "#})?;

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        import importlib

        importlib.import_module("ruff.c")
    "#})?;
    root.child("ruff").child("c.py").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn globs() -> Result<()> {
    let tempdir = TempDir::new()?;

    let root = ChildPath::new(tempdir.path());

    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        [analyze]
        include-dependencies = { "ruff/a.py" = ["ruff/b.py"], "ruff/b.py" = ["ruff/*.py"], "ruff/c.py" = ["*.json"] }
    "#})?;

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff").child("a.py").write_str("")?;
    root.child("ruff").child("b.py").write_str("")?;
    root.child("ruff").child("c.py").write_str("")?;
    root.child("ruff").child("d.json").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/__init__.py",
            "ruff/a.py",
            "ruff/b.py",
            "ruff/c.py"
          ],
          "ruff/c.py": [
            "ruff/d.json"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn exclude() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        [analyze]
        exclude = ["ruff/c.py"]
    "#})?;

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        import ruff.b
    "#})?;
    root.child("ruff").child("b.py").write_str("")?;
    root.child("ruff").child("c.py").write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn wildcard() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        from ruff.b import *
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
        from ruff import c
    "#})?;
    root.child("ruff")
        .child("c.py")
        .write_str(indoc::indoc! {r#"
        from ruff.utils import *
    "#})?;

    root.child("ruff")
        .child("utils")
        .child("__init__.py")
        .write_str("from .helpers import *")?;
    root.child("ruff")
        .child("utils")
        .child("helpers.py")
        .write_str("")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py"
          ],
          "ruff/c.py": [
            "ruff/utils/__init__.py"
          ],
          "ruff/utils/__init__.py": [
            "ruff/utils/helpers.py"
          ],
          "ruff/utils/helpers.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn nested_imports() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        match x:
            case 1:
                import ruff.b
    "#})?;
    root.child("ruff")
        .child("b.py")
        .write_str(indoc::indoc! {r#"
            try:
                import ruff.c
            except ImportError as e:
                import ruff.d
    "#})?;
    root.child("ruff")
        .child("c.py")
        .write_str(indoc::indoc! {r#"def c(): ..."#})?;
    root.child("ruff")
        .child("d.py")
        .write_str(indoc::indoc! {r#"def d(): ..."#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "ruff/__init__.py": [],
          "ruff/a.py": [
            "ruff/b.py"
          ],
          "ruff/b.py": [
            "ruff/c.py",
            "ruff/d.py"
          ],
          "ruff/c.py": [],
          "ruff/d.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Test for venv resolution with the `--python` flag.
///
/// Based on the [albatross-virtual-workspace] example from the uv repo and the report in [#16598].
///
/// [albatross-virtual-workspace]: https://github.com/astral-sh/uv/tree/aa629c4a/scripts/workspaces/albatross-virtual-workspace
/// [#16598]: https://github.com/astral-sh/ruff/issues/16598
#[test]
fn venv() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // packages
    // ├── albatross
    // │   ├── check_installed_albatross.py
    // │   ├── pyproject.toml
    // │   └── src
    // │       └── albatross
    // │           └── __init__.py
    // └── bird-feeder
    //     ├── check_installed_bird_feeder.py
    //     ├── pyproject.toml
    //     └── src
    //         └── bird_feeder
    //             └── __init__.py

    let packages = root.child("packages");

    let albatross = packages.child("albatross");
    albatross
        .child("check_installed_albatross.py")
        .write_str("from albatross import fly")?;
    albatross
        .child("pyproject.toml")
        .write_str(indoc::indoc! {r#"
        [project]
        name = "albatross"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bird-feeder", "tqdm>=4,<5"]

        [tool.uv.sources]
        bird-feeder = { workspace = true }
    "#})?;
    albatross
        .child("src")
        .child("albatross")
        .child("__init__.py")
        .write_str("import tqdm; from bird_feeder import use")?;

    let bird_feeder = packages.child("bird-feeder");
    bird_feeder
        .child("check_installed_bird_feeder.py")
        .write_str("from bird_feeder import use; from albatross import fly")?;
    bird_feeder
        .child("pyproject.toml")
        .write_str(indoc::indoc! {r#"
        [project]
        name = "bird-feeder"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>=4.3.0,<5"]
    "#})?;
    bird_feeder
        .child("src")
        .child("bird_feeder")
        .child("__init__.py")
        .write_str("import anyio")?;

    let venv = root.child(".venv");
    let bin = venv.child("bin");
    bin.child("python").touch()?;
    let home = format!("home = {}", bin.to_string_lossy());
    venv.child("pyvenv.cfg").write_str(&home)?;
    let site_packages = venv.child("lib").child("python3.12").child("site-packages");
    site_packages
        .child("_albatross.pth")
        .write_str(&albatross.join("src").to_string_lossy())?;
    site_packages
        .child("_bird_feeder.pth")
        .write_str(&bird_feeder.join("src").to_string_lossy())?;
    site_packages.child("tqdm").child("__init__.py").touch()?;

    // without `--python .venv`, the result should only include dependencies within the albatross
    // package
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(
            command().arg("packages/albatross").current_dir(&root),
            @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "packages/albatross/check_installed_albatross.py": [
            "packages/albatross/src/albatross/__init__.py"
          ],
          "packages/albatross/src/albatross/__init__.py": []
        }

        ----- stderr -----
        "#);
    });

    // with `--python .venv` both workspace and third-party dependencies are included
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(
            command().args(["--python", ".venv"]).arg("packages/albatross").current_dir(&root),
            @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "packages/albatross/check_installed_albatross.py": [
            "packages/albatross/src/albatross/__init__.py"
          ],
          "packages/albatross/src/albatross/__init__.py": [
            ".venv/lib/python3.12/site-packages/tqdm/__init__.py",
            "packages/bird-feeder/src/bird_feeder/__init__.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    // test the error message for a non-existent venv. it's important that the `ruff analyze graph`
    // flag matches the ty flag used to generate the error message (`--python`)
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(
            command().args(["--python", "none"]).arg("packages/albatross").current_dir(&root),
            @"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Invalid `--python` argument `none`: does not point to a Python executable or a directory on disk
          Cause: No such file or directory (os error 2)
        ");
    });

    Ok(())
}

#[test]
fn notebook_basic() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        def helper():
            pass
    "#})?;

    // Create a basic notebook with a simple import
    root.child("notebook.ipynb").write_str(indoc::indoc! {r#"
        {
          "cells": [
            {
              "cell_type": "code",
              "execution_count": null,
              "metadata": {},
              "outputs": [],
              "source": [
                "from ruff.a import helper"
              ]
            }
          ],
          "metadata": {
            "language_info": {
              "name": "python",
              "version": "3.12.0"
            }
          },
          "nbformat": 4,
          "nbformat_minor": 5
        }
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "notebook.ipynb": [
            "ruff/a.py"
          ],
          "ruff/__init__.py": [],
          "ruff/a.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Test that the `src` configuration option is respected.
///
/// This is useful for monorepos where there are multiple source directories that need to be
/// included in the module resolution search path.
#[test]
fn src_option() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // Create a lib directory with a package.
    root.child("lib")
        .child("mylib")
        .child("__init__.py")
        .write_str("def helper(): pass")?;

    // Create an app directory with a file that imports from mylib.
    root.child("app").child("__init__.py").write_str("")?;
    root.child("app")
        .child("main.py")
        .write_str("from mylib import helper")?;

    // Without src configured, the import from mylib won't resolve.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("app").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "app/__init__.py": [],
          "app/main.py": []
        }

        ----- stderr -----
        "#);
    });

    // With src = ["lib"], the import should resolve.
    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        src = ["lib"]
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("app").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "app/__init__.py": [],
          "app/main.py": [
            "lib/mylib/__init__.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Test that glob patterns in `src` are expanded.
#[test]
fn src_glob_expansion() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // Create multiple lib directories with packages.
    root.child("libs")
        .child("lib_a")
        .child("pkg_a")
        .child("__init__.py")
        .write_str("def func_a(): pass")?;
    root.child("libs")
        .child("lib_b")
        .child("pkg_b")
        .child("__init__.py")
        .write_str("def func_b(): pass")?;

    // Create an app that imports from both packages.
    root.child("app").child("__init__.py").write_str("")?;
    root.child("app")
        .child("main.py")
        .write_str("from pkg_a import func_a\nfrom pkg_b import func_b")?;

    // Use a glob pattern to include all lib directories.
    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        src = ["libs/*"]
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("app").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "app/__init__.py": [],
          "app/main.py": [
            "libs/lib_a/pkg_a/__init__.py",
            "libs/lib_b/pkg_b/__init__.py"
          ]
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

#[test]
fn notebook_with_magic() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        def helper():
            pass
    "#})?;

    // Create a notebook with IPython magic commands and imports
    root.child("notebook.ipynb").write_str(indoc::indoc! {r#"
        {
          "cells": [
            {
              "cell_type": "code",
              "execution_count": null,
              "metadata": {},
              "outputs": [],
              "source": [
                "%load_ext autoreload\n",
                "%autoreload 2"
              ]
            },
            {
              "cell_type": "code",
              "execution_count": null,
              "metadata": {},
              "outputs": [],
              "source": [
                "from ruff.a import helper"
              ]
            }
          ],
          "metadata": {
            "language_info": {
              "name": "python",
              "version": "3.12.0"
            }
          },
          "nbformat": 4,
          "nbformat_minor": 5
        }
    "#})?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "notebook.ipynb": [
            "ruff/a.py"
          ],
          "ruff/__init__.py": [],
          "ruff/a.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// In a large monorepo, there might be multiple packages with the same name in different
/// directories. Imports should NOT resolve to these unrelated packages just because they
/// share a name.
#[test]
fn no_cross_package_resolution() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // Create a main application that imports "foo"
    root.child("ruff").child("__init__.py").write_str("")?;
    root.child("ruff")
        .child("main.py")
        .write_str(indoc::indoc! {r#"
        import foo  # This is meant to be a third-party library, not resolved locally
    "#})?;

    // Create an unrelated "foo" module in a completely different part of the repo
    root.child("other")
        .child("tools")
        .child("foo")
        .child("__init__.py")
        .write_str("# Unrelated internal tool")?;

    // The import should NOT resolve to the unrelated other/tools/foo module
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "other/tools/foo/__init__.py": [],
          "ruff/__init__.py": [],
          "ruff/main.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Test that imports in one package don't resolve to modules in sibling packages
/// unless explicitly configured.
#[test]
fn no_sibling_package_resolution() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // Create two sibling packages that are independent
    root.child("packages")
        .child("alpha")
        .child("__init__.py")
        .write_str("")?;
    root.child("packages")
        .child("alpha")
        .child("main.py")
        .write_str(indoc::indoc! {r#"
        import bar  # Intended to be a third-party library
    "#})?;

    root.child("packages")
        .child("beta")
        .child("__init__.py")
        .write_str("")?;
    root.child("packages")
        .child("beta")
        .child("bar")
        .child("__init__.py")
        .write_str("# Beta-specific module")?;

    // alpha's `import bar` should NOT resolve to beta's bar module
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("packages/alpha").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "packages/alpha/__init__.py": [],
          "packages/alpha/main.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}

/// Test that when src is configured, imports resolve correctly
/// but still don't leak across to unrelated packages.
#[test]
fn configured_src_no_cross_resolution() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = ChildPath::new(tempdir.path());

    // Configure src to include only the lib directory
    root.child("ruff.toml").write_str(indoc::indoc! {r#"
        src = ["lib"]
    "#})?;

    // Create the main library
    root.child("lib")
        .child("ruff")
        .child("__init__.py")
        .write_str("")?;
    root.child("lib")
        .child("ruff")
        .child("a.py")
        .write_str(indoc::indoc! {r#"
        from ruff import b
        import baz  # Third-party, should not resolve
    "#})?;
    root.child("lib")
        .child("ruff")
        .child("b.py")
        .write_str("")?;

    // Create an unrelated "baz" module outside the configured src
    root.child("other")
        .child("baz")
        .child("__init__.py")
        .write_str("# Unrelated module")?;

    // `from ruff import b` should resolve (same package)
    // `import baz` should NOT resolve (not in configured src)
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("lib").current_dir(&root), @r#"
        success: true
        exit_code: 0
        ----- stdout -----
        {
          "lib/ruff/__init__.py": [],
          "lib/ruff/a.py": [
            "lib/ruff/b.py"
          ],
          "lib/ruff/b.py": []
        }

        ----- stderr -----
        "#);
    });

    Ok(())
}
