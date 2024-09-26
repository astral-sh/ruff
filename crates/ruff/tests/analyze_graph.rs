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
