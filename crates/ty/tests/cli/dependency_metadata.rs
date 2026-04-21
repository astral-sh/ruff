use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

fn write_dependency_metadata(case: &CliTest, dependencies: &str) -> anyhow::Result<()> {
    let root = format!("{:?}", case.root().to_str().unwrap());
    case.write_file(
        "metadata.json",
        &format!(
            r#"
            {{
              "schema": {{"version": "preview"}},
              "members": [
                {{"name": "app", "path": {root}, "id": "app"}}
              ],
              "resolution": {{
                "app": {{"name": "app", "dependencies": {dependencies}}},
                "requests==2.32.0@registry+https://pypi.org/simple": {{"name": "requests", "dependencies": []}}
              }},
              "module_owners": {{
                "requests": ["requests==2.32.0@registry+https://pypi.org/simple"]
              }}
            }}
            "#
        ),
    )
}

#[test]
fn dependency_metadata_enables_missing_direct_dependency_lint() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", "import requests"),
        (
            ".venv/lib/python3.13/site-packages/requests/__init__.py",
            "",
        ),
        (".venv/Lib/site-packages/requests/__init__.py", ""),
    ])?;

    write_dependency_metadata(&case, "[]")?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `requests` is used but no direct dependency on `requests` is declared
     --> test.py:1:8
      |
    1 | import requests
      |        ^^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn declared_dependency_is_not_reported() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", "import requests"),
        (
            ".venv/lib/python3.13/site-packages/requests/__init__.py",
            "",
        ),
        (".venv/Lib/site-packages/requests/__init__.py", ""),
    ])?;

    write_dependency_metadata(
        &case,
        r#"[{"id": "requests==2.32.0@registry+https://pypi.org/simple"}]"#,
    )?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn dependency_metadata_applies_with_multiple_overrides() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["*.py"]

            [tool.ty.overrides.analysis]
            respect-type-ignore-comments = true

            [[tool.ty.overrides]]
            include = ["test.py"]

            [tool.ty.overrides.rules]
            division-by-zero = "ignore"
            "#,
        ),
        ("test.py", "import requests"),
        (
            ".venv/lib/python3.13/site-packages/requests/__init__.py",
            "",
        ),
        (".venv/Lib/site-packages/requests/__init__.py", ""),
    ])?;

    write_dependency_metadata(&case, "[]")?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `requests` is used but no direct dependency on `requests` is declared
     --> test.py:1:8
      |
    1 | import requests
      |        ^^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}
