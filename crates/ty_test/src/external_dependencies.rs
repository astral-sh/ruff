use crate::db::Db;

use anyhow::{Context, Result, anyhow, bail};
use ruff_db::system::{DbWithWritableSystem as _, SystemPath};
use ruff_python_ast::PythonVersion;

/// Setup a virtual environment in the in-memory filesystem of `db` with
/// the specified dependencies installed.
pub(crate) fn setup_venv(
    db: &mut Db,
    dependencies: &[String],
    python_version: PythonVersion,
    dest_venv_path: &SystemPath,
) -> Result<()> {
    // Create a temporary directory for the project
    let temp_dir = tempfile::Builder::new()
        .prefix("mdtest-venv-")
        .tempdir()
        .context("Failed to create temporary directory for mdtest virtual environment")?;

    let temp_path = SystemPath::from_std_path(temp_dir.path())
        .ok_or_else(|| {
            anyhow!(
                "Temporary directory path is not valid UTF-8: {}",
                temp_dir.path().display()
            )
        })?
        .to_path_buf();

    // Generate a minimal pyproject.toml
    let pyproject_toml = format!(
        r#"[project]
name = "mdtest-deps"
version = "0.1.0"
requires-python = "~={python_version}.0"
dependencies = [
{deps}
]
"#,
        python_version = python_version,
        deps = dependencies
            .iter()
            .map(|dep| format!("    \"{dep}\","))
            .collect::<Vec<_>>()
            .join("\n")
    );

    std::fs::write(
        temp_path.join("pyproject.toml").as_std_path(),
        pyproject_toml,
    )
    .context("Failed to write pyproject.toml")?;

    // Run `uv sync` to install dependencies
    let uv_sync_output = std::process::Command::new("uv")
        .arg("sync")
        .current_dir(temp_path.as_std_path())
        .output()
        .context("Failed to run `uv sync`. Is `uv` installed?")?;

    if !uv_sync_output.status.success() {
        let stderr = String::from_utf8_lossy(&uv_sync_output.stderr);
        bail!(
            "`uv sync` failed with exit code {:?}:\n{}",
            uv_sync_output.status.code(),
            stderr
        );
    }

    let venv_path = temp_path.join(".venv");

    copy_site_packages_to_db(db, &venv_path, dest_venv_path, python_version)
}

/// Copy the site-packages directory from a real virtual environment to the in-memory filesystem of `db`.
///
/// This recursively copies all files from the venv's site-packages directory into the
/// in-memory filesystem at the specified destination path.
fn copy_site_packages_to_db(
    db: &mut Db,
    venv_path: &SystemPath,
    dest_venv_path: &SystemPath,
    python_version: PythonVersion,
) -> Result<()> {
    use std::fs;

    // Determine the site-packages path within the venv
    let (site_packages_path, python_dir_name) = if cfg!(target_os = "windows") {
        let sp_path = venv_path.join("Lib").join("site-packages");
        (sp_path, String::new()) // Windows doesn't need the python version in the path
    } else {
        // On Unix, we need to find the actual python directory in lib/
        // It could be python3.12, python3.12.1, etc.
        let lib_path = venv_path.join("lib");

        if !lib_path.as_std_path().exists() {
            bail!("'lib' directory not found in virtual environment at '{venv_path}'",);
        }

        // Find the python directory
        let major = python_version.major;
        let minor = python_version.minor;
        let python_dir = fs::read_dir(lib_path.as_std_path())
            .context("Failed to read 'lib' directory")?
            .filter_map(Result::ok)
            .find(|entry| {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                name_str == format!("python{major}.{minor}")
                    && entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow!(
                    "Could not find python directory in '{}'. Expected 'python{}.{}'",
                    lib_path,
                    python_version.major,
                    python_version.minor
                )
            })?;

        let python_dir_name = python_dir.file_name();
        let python_dir_str = python_dir_name
            .to_str()
            .ok_or_else(|| anyhow!("Python directory name is not valid UTF-8"))?;

        let actual_python_dir = lib_path.join(python_dir_str);
        (
            actual_python_dir.join("site-packages"),
            python_dir_str.to_string(),
        )
    };

    if !site_packages_path.as_std_path().exists() {
        bail!("site-packages directory not found at '{site_packages_path}'",);
    }

    // Destination site-packages path in the in-memory filesystem
    // Use the actual python directory name we found
    let dest_site_packages = if cfg!(target_os = "windows") {
        dest_venv_path.join("Lib").join("site-packages")
    } else {
        dest_venv_path
            .join("lib")
            .join(&python_dir_name)
            .join("site-packages")
    };

    // Create the destination directory structure
    db.create_directory_all(&dest_site_packages)
        .context("Failed to create site-packages directory in database")?;

    // Recursively copy all files from site-packages
    copy_directory_recursive(db, &site_packages_path, &dest_site_packages)?;

    Ok(())
}

fn copy_directory_recursive(db: &mut Db, src: &SystemPath, dest: &SystemPath) -> Result<()> {
    use std::fs;

    for entry in fs::read_dir(src.as_std_path())
        .with_context(|| format!("Failed to read directory {src}"))?
    {
        let entry = entry.with_context(|| format!("Failed to read directory entry in {src}"))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to get file type for {}", entry_path.display()))?;

        let src_path = SystemPath::from_std_path(&entry_path)
            .ok_or_else(|| anyhow!("Path {} is not valid UTF-8", entry_path.display()))?;

        let file_name = entry.file_name();
        let file_name_str = file_name.to_str().ok_or_else(|| {
            anyhow!(
                "File name {} is not valid UTF-8",
                file_name.to_string_lossy()
            )
        })?;

        let dest_path = dest.join(file_name_str);

        if file_type.is_dir() {
            // Skip __pycache__ directories and other unnecessary directories
            if file_name_str == "__pycache__" || file_name_str.ends_with(".dist-info") {
                continue;
            }

            db.create_directory_all(&dest_path)
                .with_context(|| format!("Failed to create directory {dest_path}"))?;

            copy_directory_recursive(db, src_path, &dest_path)?;
        } else if file_type.is_file() {
            let is_python_source = entry_path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyi")
            });

            if !is_python_source {
                // Skip all non-Python files (binaries, data files, etc.)
                continue;
            }

            let contents = fs::read_to_string(src_path.as_std_path())
                .with_context(|| format!("Failed to read file {src_path}"))?;

            db.write_file(&dest_path, contents)
                .with_context(|| format!("Failed to write file {dest_path}"))?;
        }
    }

    Ok(())
}
