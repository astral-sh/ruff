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

fn write_site_package(case: &CliTest, module: &str) -> anyhow::Result<()> {
    let path = format!(".venv/lib/python3.13/site-packages/{module}/__init__.py");
    case.write_file(path.as_str(), "")?;
    let path = format!(".venv/Lib/site-packages/{module}/__init__.py");
    case.write_file(path.as_str(), "")?;
    Ok(())
}

fn write_dependency_group_metadata(
    case: &CliTest,
    group_dependencies: &str,
    module_owners: &str,
) -> anyhow::Result<()> {
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
                "app": {{
                  "name": "app",
                  "dependencies": [],
                  "dependency_groups": [
                    {{"name": "dev", "id": "app:dev"}}
                  ]
                }},
                "app:dev": {{"dependencies": {group_dependencies}}},
                "inline-snapshot==0.20.8@registry+https://pypi.org/simple": {{"name": "inline-snapshot", "dependencies": []}},
                "pyx-test==1.0.0@registry+https://pypi.org/simple": {{
                  "name": "pyx-test",
                  "dependencies": [
                    {{"id": "pytest==8.0.0@registry+https://pypi.org/simple"}}
                  ]
                }},
                "pytest==8.0.0@registry+https://pypi.org/simple": {{"name": "pytest", "dependencies": []}}
              }},
              "module_owners": {{
                {module_owners}
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
fn dependency_metadata_infers_editable_module_owners() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", "import lib_module"),
        ("libs/lib/src/lib_module/__init__.py", ""),
    ])?;
    let root = format!("{:?}", case.root().to_str().unwrap());
    let lib = format!("{:?}", case.root().join("libs/lib").to_str().unwrap());
    let editable = case.root().join("libs/lib/src");
    let editable = editable.to_str().unwrap();

    case.write_file(".venv/lib/python3.13/site-packages/_lib.pth", editable)?;
    case.write_file(".venv/Lib/site-packages/_lib.pth", editable)?;
    case.write_file(
        "metadata.json",
        &format!(
            r#"
            {{
              "schema": {{"version": "preview"}},
              "members": [
                {{"name": "app", "path": {root}, "id": "app"}},
                {{"name": "lib-project", "path": {lib}, "id": "lib-project"}}
              ],
              "resolution": {{
                "app": {{"name": "app", "dependencies": []}},
                "lib-project": {{"name": "lib-project", "dependencies": []}}
              }},
              "module_owners": {{}}
            }}
            "#
        ),
    )?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `lib_module` is used but no direct dependency on `lib-project` is declared
     --> test.py:1:8
      |
    1 | import lib_module
      |        ^^^^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn dependency_group_dependency_is_reported_in_package_code() -> anyhow::Result<()> {
    let case = CliTest::with_files([("src/app/__init__.py", "import inline_snapshot")])?;
    write_site_package(&case, "inline_snapshot")?;
    write_dependency_group_metadata(
        &case,
        r#"[{"id": "inline-snapshot==0.20.8@registry+https://pypi.org/simple"}]"#,
        r#"
                "app": ["app"],
                "inline_snapshot": ["inline-snapshot==0.20.8@registry+https://pypi.org/simple"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `inline_snapshot` is used but no direct dependency on `inline-snapshot` is declared
     --> src/app/__init__.py:1:8
      |
    1 | import inline_snapshot
      |        ^^^^^^^^^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn dependency_group_dependency_is_allowed_in_non_package_file() -> anyhow::Result<()> {
    let case = CliTest::with_files([("tests/test_app.py", "import inline_snapshot")])?;
    write_site_package(&case, "inline_snapshot")?;
    write_dependency_group_metadata(
        &case,
        r#"[{"id": "inline-snapshot==0.20.8@registry+https://pypi.org/simple"}]"#,
        r#"
                "app": ["app"],
                "inline_snapshot": ["inline-snapshot==0.20.8@registry+https://pypi.org/simple"]
        "#,
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
fn dependency_group_dependency_does_not_allow_transitive_dependency() -> anyhow::Result<()> {
    let case = CliTest::with_files([("tests/test_app.py", "import pytest")])?;
    write_site_package(&case, "pytest")?;
    write_dependency_group_metadata(
        &case,
        r#"[{"id": "pyx-test==1.0.0@registry+https://pypi.org/simple"}]"#,
        r#""pytest": ["pytest==8.0.0@registry+https://pypi.org/simple"]"#,
    )?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `pytest` is used but no direct dependency on `pytest` is declared
     --> tests/test_app.py:1:8
      |
    1 | import pytest
      |        ^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn dependency_group_dependency_is_reported_in_editable_package_code() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "ty.toml",
            r#"
            [environment]
            root = ["tests"]
            "#,
        ),
        ("src/app/__init__.py", "import inline_snapshot"),
        ("tests/__init__.py", ""),
    ])?;
    let editable = case.root().join("src");
    let editable = editable.to_str().unwrap();
    case.write_file(".venv/lib/python3.13/site-packages/_app.pth", editable)?;
    case.write_file(".venv/Lib/site-packages/_app.pth", editable)?;
    write_site_package(&case, "inline_snapshot")?;
    write_dependency_group_metadata(
        &case,
        r#"[{"id": "inline-snapshot==0.20.8@registry+https://pypi.org/simple"}]"#,
        r#""inline_snapshot": ["inline-snapshot==0.20.8@registry+https://pypi.org/simple"]"#,
    )?;

    assert_cmd_snapshot!(case.command()
        .arg("--python").arg(".venv")
        .arg("--dependency-metadata").arg("metadata.json")
        .arg("src/app/__init__.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[missing-direct-dependency]: Third-party import `inline_snapshot` is used but no direct dependency on `inline-snapshot` is declared
     --> src/app/__init__.py:1:8
      |
    1 | import inline_snapshot
      |        ^^^^^^^^^^^^^^^
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
