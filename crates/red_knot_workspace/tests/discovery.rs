//! Integration tests for workspace discovery

use anyhow::Context;
use insta::assert_debug_snapshot;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::system::{SystemPathBuf, TestSystem};

#[test]
fn package_without_pyproject() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([(root.join("src/foo.py"), ""), (root.join("src/bar.py"), "")])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None)
        .context("Failed to discover workspace")?;

    assert_eq!(workspace.root(), &*root);

    assert_debug_snapshot!(&workspace);

    Ok(())
}

#[test]
fn single_package() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "backend"
            "#,
            ),
        ])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None)
        .context("Failed to discover workspace")?;

    assert_eq!(workspace.root(), &*root);
    assert_debug_snapshot!(&workspace);

    // Discovering the same package from a subdirectory should give the same result
    let from_src = WorkspaceMetadata::discover(&root.join("src"), &system, None)
        .context("Failed to discover workspace from src sub-directory")?;

    assert_eq!(from_src, workspace);

    Ok(())
}

#[test]
fn workspace_members() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
                exclude = ["members/excluded"]
            "#,
            ),
            (
                root.join("members/a/pyproject.toml"),
                r#"
                [project]
                name = "member-a"
                "#,
            ),
            (
                root.join("members/x/pyproject.toml"),
                r#"
                [project]
                name = "member-x"
                "#,
            ),
        ])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None)
        .context("Failed to discover workspace")?;

    assert_eq!(workspace.root(), &*root);
    assert_debug_snapshot!(&workspace);

    // Discovering the same package from a member should give the same result
    let from_src = WorkspaceMetadata::discover(&root.join("members/a"), &system, None)
        .context("Failed to discover workspace from src sub-directory")?;

    assert_eq!(from_src, workspace);

    Ok(())
}

#[test]
fn workspace_excluded() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
                exclude = ["members/excluded"]
            "#,
            ),
            (
                root.join("members/a/pyproject.toml"),
                r#"
                [project]
                name = "member-a"
                "#,
            ),
            (
                root.join("members/excluded/pyproject.toml"),
                r#"
                [project]
                name = "member-x"
                "#,
            ),
        ])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None)
        .context("Failed to discover workspace")?;

    assert_eq!(workspace.root(), &*root);
    assert_debug_snapshot!(&workspace);

    // Discovering the `workspace` for `excluded` should discover a single-package workspace
    let excluded_workspace =
        WorkspaceMetadata::discover(&root.join("members/excluded"), &system, None)
            .context("Failed to discover workspace from src sub-directory")?;

    assert_ne!(excluded_workspace, workspace);

    Ok(())
}

#[test]
fn workspace_non_unique_member_names() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
            "#,
            ),
            (
                root.join("members/a/pyproject.toml"),
                r#"
                [project]
                name = "member"
                "#,
            ),
            (
                root.join("members/b/pyproject.toml"),
                r#"
                [project]
                name = "member"
                "#,
            ),
        ])
        .context("Failed to write files")?;

    let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
        "Discovery should error because the workspace contains two members with the same names.",
    );

    assert_eq!(
        error.to_string(),
        "The workspace contains two packages named 'member': '/app/members/a' and '/app/members/b'"
    );

    Ok(())
}

#[test]
fn nested_workspaces() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
            "#,
            ),
            (
                root.join("members/a/pyproject.toml"),
                r#"
                [project]
                name = "nested-workspace"

                [tool.knot.workspace]
                members = ["members/*"]
                "#,
            ),
        ])
        .context("Failed to write files")?;

    let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
        "Discovery should error because the workspace has a member that itself is a workspace",
    );

    assert_eq!(
        error.to_string(),
        "Workspace member '/app/members/a' is a workspace itself. Nested workspaces aren't supported"
    );

    Ok(())
}

#[test]
fn member_missing_pyproject_toml() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
            "#,
            ),
            (root.join("members/a/test.py"), ""),
        ])
        .context("Failed to write files")?;

    let error = WorkspaceMetadata::discover(&root, &system, None)
        .expect_err("Discovery should error because member `a` has no `pypyroject.toml`");

    assert_eq!(
        error.to_string(),
        "pyproject.toml for member '/app/members/a' is missing"
    );

    Ok(())
}

/// Folders that match the members pattern but don't have a pyproject.toml
/// aren't valid members and discovery fails. However, don't fail
/// if the folder name indicates that it is a hidden folder that might
/// have been crated by another tool
#[test]
fn member_pattern_matching_hidden_folder() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
            "#,
            ),
            (root.join("members/.hidden/a.py"), ""),
        ])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None);

    assert_debug_snapshot!(&workspace);

    Ok(())
}

#[test]
fn member_pattern_matching_file() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["members/*"]
            "#,
            ),
            (root.join("members/.DS_STORE"), ""),
        ])
        .context("Failed to write files")?;

    let workspace = WorkspaceMetadata::discover(&root, &system, None);

    assert_debug_snapshot!(&workspace);

    Ok(())
}

#[test]
fn workspace_root_not_an_ancestor_of_member() -> anyhow::Result<()> {
    let system = TestSystem::default();
    let root = SystemPathBuf::from("/app");

    system
        .memory_file_system()
        .write_files([
            (root.join("src/foo.py"), ""),
            (root.join("src/bar.py"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"

                [tool.knot.workspace]
                members = ["../members/*"]
            "#,
            ),
            (
                root.join("../members/a/pyproject.toml"),
                r#"
                [project]
                name = "a"
                "#,
            ),
        ])
        .context("Failed to write files")?;

    let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
        "Discovery should error because member `a` is outside the workspace's directory`",
    );

    assert_eq!(
        error.to_string(),
        "Workspace member 'a' located at '/members/a is outside the workspace '/app'"
    );

    Ok(())
}
