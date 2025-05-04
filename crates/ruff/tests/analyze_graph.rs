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

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().current_dir(&root), @r###"
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
                "ruff/e.py"
              ],
              "ruff/e.py": []
            }

            ----- stderr -----
            "###);
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
        assert_cmd_snapshot!(command().arg("--direction").arg("dependents").current_dir(&root), @r###"
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
            "###);
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
        assert_cmd_snapshot!(command().current_dir(&root), @r###"
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
            "###);
    });

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec(),
    }, {
        assert_cmd_snapshot!(command().arg("--detect-string-imports").current_dir(&root), @r###"
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
            "###);
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
        assert_cmd_snapshot!(command().current_dir(&root), @r###"
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
        "###);
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
        assert_cmd_snapshot!(command().current_dir(&root), @r###"
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
        "###);
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
        assert_cmd_snapshot!(command().current_dir(&root), @r###"
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
        "###);
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
            @r"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        ruff failed
          Cause: Invalid search path settings
          Cause: Failed to discover the site-packages directory: Invalid `--python` argument: `none` could not be canonicalized
        ");
    });

    Ok(())
}
