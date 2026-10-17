use crate::db::Db;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8Path;
use ruff_db::files::File;
use ruff_db::system::{DbWithWritableSystem as _, OsSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;
use tempfile::TempDir;
use ty_python_core::platform::PythonPlatform;
use ty_python_semantic::{PythonEnvironment, SysPrefixPathOrigin};

/// Retains one installed environment for a Markdown file, never its test database contents.
#[derive(Default)]
pub(crate) struct VenvCache {
    enabled: bool,
    entry: Option<CachedVenv>,
}

impl VenvCache {
    pub(crate) fn new() -> Self {
        Self {
            enabled: !std::env::var("MDTEST_REUSE_VENV").is_ok_and(|value| value == "0"),
            ..Self::default()
        }
    }

    fn key(
        &self,
        project: &str,
        platform: &str,
        lockfile: &Utf8Path,
        upgrade: bool,
    ) -> Option<VenvKey> {
        if !self.enabled || upgrade {
            return None;
        }
        Some(VenvKey {
            project: project.to_owned(),
            platform: platform.to_owned(),
            lockfile: std::fs::read(lockfile).ok()?,
        })
    }

    /// A matching installation still needs a fresh copy into the current test's filesystem.
    fn copy_if_matching(
        &mut self,
        db: &mut Db,
        key: &VenvKey,
        destination: &SystemPath,
    ) -> Result<bool> {
        let Some(entry) = self.entry.as_ref().filter(|entry| &entry.key == key) else {
            self.entry = None;
            return Ok(false);
        };

        if let Err(error) = copy_site_packages_to_db(db, &entry.path, destination) {
            self.entry = None;
            return Err(error);
        }
        Ok(true)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct VenvKey {
    project: String,
    platform: String,
    lockfile: Vec<u8>,
}

struct CachedVenv {
    key: VenvKey,
    path: SystemPathBuf,
    _directory: TempDir,
}

/// Local, editable, Git, and unknown sources can change without changing the lockfile.
fn reusable_lockfile(bytes: &[u8]) -> bool {
    let Ok(contents) = std::str::from_utf8(bytes) else {
        return false;
    };
    let Ok(lockfile) = toml::from_str::<toml::Table>(contents) else {
        return false;
    };
    if lockfile.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return false;
    }
    let Some(packages) = lockfile.get("package").and_then(toml::Value::as_array) else {
        return false;
    };
    let mut found_root = false;
    for package in packages {
        let Some(source) = package.get("source").and_then(toml::Value::as_table) else {
            return false;
        };
        if source.len() != 1 {
            return false;
        }
        if source
            .get("registry")
            .and_then(toml::Value::as_str)
            .is_some_and(|registry| {
                registry.starts_with("https://") || registry.starts_with("http://")
            })
        {
            continue;
        }
        if !found_root
            && source.get("virtual").and_then(toml::Value::as_str) == Some(".")
            && package.get("name").and_then(toml::Value::as_str) == Some("mdtest-deps")
            && package.get("version").and_then(toml::Value::as_str) == Some("0.1.0")
        {
            found_root = true;
        } else {
            return false;
        }
    }
    found_root
}

/// Setup a virtual environment in the in-memory filesystem of `db` with
/// the specified dependencies installed.
pub(crate) fn setup_venv(
    db: &mut Db,
    dependencies: &[String],
    python_version: PythonVersion,
    python_platform: &PythonPlatform,
    dest_venv_path: &SystemPath,
    lockfile_path: &Utf8Path,
    cache: &mut VenvCache,
) -> Result<()> {
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

    // Convert PythonPlatform to uv's platform format
    let uv_platform = match python_platform {
        PythonPlatform::Identifier(id) => match id.as_str() {
            "win32" => "windows",
            "darwin" => "macos",
            "linux" => "linux",
            other => other,
        },
        PythonPlatform::All => {
            bail!("For an mdtest with external dependencies, a Python platform must be specified");
        }
    };

    let upgrade_lockfile = std::env::var("MDTEST_UPGRADE_LOCKFILES").is_ok_and(|v| v == "1");
    let key = cache.key(
        &pyproject_toml,
        uv_platform,
        lockfile_path,
        upgrade_lockfile,
    );
    if let Some(key) = &key
        && cache.copy_if_matching(db, key, dest_venv_path)?
    {
        return Ok(());
    }
    cache.entry = None;
    let key = key.filter(|key| reusable_lockfile(&key.lockfile));

    let temp_dir = tempfile::Builder::new()
        .prefix("mdtest-venv-")
        .tempdir()
        .context("Failed to create temporary directory for mdtest virtual environment")?;

    // Canonicalize here to fix problems with `.strip_prefix()` later on Windows.
    let temp_dir_path = dunce::canonicalize(temp_dir.path())
        .context("Failed to canonicalize temporary directory path")?;
    let temp_path = SystemPath::from_std_path(&temp_dir_path)
        .ok_or_else(|| {
            anyhow!(
                "Temporary directory path is not valid UTF-8: {}",
                temp_dir_path.display()
            )
        })?
        .to_path_buf();
    std::fs::write(
        temp_path.join("pyproject.toml").as_std_path(),
        pyproject_toml,
    )
    .context("Failed to write pyproject.toml")?;

    let use_locked = if upgrade_lockfile {
        // In upgrade mode, we'll generate a new lockfile
        false
    } else if let Some(key) = &key {
        // Install exactly the bytes used for the cache key, even if the source lockfile changes.
        std::fs::write(temp_path.join("uv.lock").as_std_path(), &key.lockfile)
            .with_context(|| format!("Failed to copy lockfile from '{lockfile_path}'"))?;
        true
    } else if lockfile_path.exists() {
        // Copy existing lockfile to temp directory
        let temp_lockfile = temp_path.join("uv.lock");
        std::fs::copy(lockfile_path, temp_lockfile.as_std_path())
            .with_context(|| format!("Failed to copy lockfile from '{lockfile_path}'"))?;
        true
    } else {
        // No existing lockfile - error in normal mode
        bail!(
            "Lockfile not found at '{lockfile_path}'. Run with `MDTEST_UPGRADE_LOCKFILES=1` to generate it.",
        );
    };

    // Run `uv sync` to install dependencies
    let mut uv_sync = std::process::Command::new("uv");
    uv_sync
        .args(["sync", "--python-platform", uv_platform])
        .current_dir(temp_path.as_std_path());
    if use_locked {
        uv_sync.arg("--locked");
    }

    let uv_sync_output = uv_sync
        .output()
        .context("Failed to run `uv sync`. Is `uv` installed?")?;

    if !uv_sync_output.status.success() {
        let stderr = String::from_utf8_lossy(&uv_sync_output.stderr);

        if use_locked
            && stderr.contains("`uv.lock` needs to be updated, but `--locked` was provided.")
        {
            bail!(
                "Lockfile is out of date. Use one of these commands to regenerate it:\n\
                 \n\
                 uv run crates/ty_python_semantic/mdtest.py -e external/\n\
                 \n\
                 Or using cargo:\n\
                 \n\
                 MDTEST_EXTERNAL=1 MDTEST_UPGRADE_LOCKFILES=1 cargo test -p ty_python_semantic --test mdtest mdtest__external"
            );
        }

        bail!(
            "`uv sync` failed with exit code {:?}:\n{}",
            uv_sync_output.status.code(),
            stderr
        );
    }

    // In upgrade mode, copy the generated lockfile back to the source location
    if upgrade_lockfile {
        let temp_lockfile = temp_path.join("uv.lock");
        let temp_lockfile = temp_lockfile.as_std_path();
        if temp_lockfile.exists() {
            std::fs::copy(temp_lockfile, lockfile_path)
                .with_context(|| format!("Failed to write lockfile to '{lockfile_path}'"))?;
        } else {
            bail!(
                "Expected uv to create a lockfile at '{}'",
                temp_lockfile.display()
            );
        }
    }

    let venv_path = temp_path.join(".venv");

    copy_site_packages_to_db(db, &venv_path, dest_venv_path)?;
    if let Some(key) = key {
        cache.entry = Some(CachedVenv {
            key,
            path: venv_path,
            _directory: temp_dir,
        });
    }
    Ok(())
}

/// Copy the site-packages directory from a real virtual environment to the in-memory filesystem of `db`.
///
/// This recursively copies all files from the venv's site-packages directory into the
/// in-memory filesystem at the specified destination path.
fn copy_site_packages_to_db(
    db: &mut Db,
    venv_path: &SystemPath,
    dest_venv_path: &SystemPath,
) -> Result<()> {
    // Discover the site-packages directory in the virtual environment
    let system = OsSystem::new(venv_path);
    let env = PythonEnvironment::new(venv_path, SysPrefixPathOrigin::LocalVenv, &system)
        .context("Failed to create Python environment for temporary virtual environment")?;

    let site_packages_paths = env
        .site_packages_paths(&system)
        .context(format!("Failed to discover site-packages in '{venv_path}'"))?;

    let site_packages_path = site_packages_paths
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No site-packages directory found in '{venv_path}'"))?;

    // Create the destination directory structure
    let relative_site_packages = site_packages_path.strip_prefix(venv_path).map_err(|_| {
        anyhow!("site-packages path '{site_packages_path}' is not under venv path '{venv_path}'")
    })?;
    let dest_site_packages = dest_venv_path.join(relative_site_packages);
    db.create_directory_all(&dest_site_packages)
        .context("Failed to create site-packages directory in database")?;

    // Recursively copy all files from site-packages
    copy_directory_recursive(db, &site_packages_path, &dest_site_packages)?;

    File::sync_path(db, &dest_site_packages);

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

#[cfg(test)]
mod tests {
    use std::fs;

    use ruff_db::Db as _;

    use super::*;

    const LOCKFILE: &str = r#"
version = 1
[[package]]
name = "mdtest-deps"
version = "0.1.0"
source = { virtual = "." }
[[package]]
name = "dependency"
version = "1.0"
source = { registry = "https://pypi.org/simple" }
"#;

    #[test]
    fn reuse_requires_matching_immutable_inputs() -> Result<()> {
        assert!(reusable_lockfile(LOCKFILE.as_bytes()));
        for source in [
            r#"path = "../dependency""#,
            r#"editable = "../dependency""#,
            r#"git = "https://example.org/repo#revision""#,
            r#"virtual = "../dependency""#,
            r#"registry = "file:///tmp/index""#,
            r#"registry = "https://pypi.org/simple", path = "../dependency""#,
            "malformed =",
        ] {
            let lockfile = LOCKFILE.replace(r#"registry = "https://pypi.org/simple""#, source);
            assert!(!reusable_lockfile(lockfile.as_bytes()), "{source}");
        }

        let directory = TempDir::new()?;
        let lockfile = directory.path().join("test.lock");
        let lockfile = Utf8Path::from_path(&lockfile).context("UTF-8 test path")?;
        fs::write(lockfile, LOCKFILE)?;
        let mut cache = VenvCache {
            enabled: true,
            ..VenvCache::default()
        };
        let project = "requires-python = '~=3.13.0'\ndependencies = ['dependency==1.0']";
        let key = cache
            .key(project, "linux", lockfile, false)
            .context("key")?;
        assert!(cache.key(project, "linux", lockfile, true).is_none());
        cache.enabled = false;
        assert!(cache.key(project, "linux", lockfile, false).is_none());
        cache.enabled = true;

        let mut db = Db::setup();
        for (project, platform, contents) in [
            (
                project.replace("3.13", "3.12"),
                "linux",
                LOCKFILE.to_owned(),
            ),
            (project.replace("1.0", "2.0"), "linux", LOCKFILE.to_owned()),
            (project.to_owned(), "windows", LOCKFILE.to_owned()),
            (
                project.to_owned(),
                "linux",
                format!("{LOCKFILE}# changed\n"),
            ),
        ] {
            fs::write(lockfile, contents)?;
            let changed = cache
                .key(&project, platform, lockfile, false)
                .context("key")?;
            let entry_directory = TempDir::new()?;
            cache.entry = Some(CachedVenv {
                key: key.clone(),
                path: SystemPath::from_std_path(entry_directory.path())
                    .context("UTF-8 test path")?
                    .to_path_buf(),
                _directory: entry_directory,
            });
            assert!(!cache.copy_if_matching(&mut db, &changed, SystemPath::new("/.venv"))?);
            assert!(cache.entry.is_none());
        }
        Ok(())
    }

    #[test]
    fn reuse_copies_fresh_files_and_propagates_copy_failures() -> Result<()> {
        let directory = TempDir::new()?;
        let path = dunce::canonicalize(directory.path())?;
        let path = SystemPath::from_std_path(&path)
            .context("UTF-8 test path")?
            .to_path_buf();
        let relative_site_packages = if cfg!(windows) {
            "Lib/site-packages"
        } else {
            "lib/python3.13/site-packages"
        };
        let site_packages = path.join(relative_site_packages);
        fs::create_dir_all(&site_packages)?;
        fs::write(
            path.join("pyvenv.cfg"),
            format!("home = {path}\nversion = 3.13.0\n"),
        )?;
        fs::write(site_packages.join("dependency.py"), "value = 1\n")?;
        let key = VenvKey {
            project: String::new(),
            platform: "linux".to_owned(),
            lockfile: LOCKFILE.as_bytes().to_vec(),
        };
        let mut cache = VenvCache {
            entry: Some(CachedVenv {
                key: key.clone(),
                path,
                _directory: directory,
            }),
            ..VenvCache::default()
        };
        let mut db = Db::setup();
        let destination = SystemPath::new("/.venv");
        let copied = destination
            .join(relative_site_packages)
            .join("dependency.py");
        assert!(cache.copy_if_matching(&mut db, &key, destination)?);
        db.write_file(&copied, "value = 2\n")?;
        db.write_file(destination.join("test_only.py"), "")?;

        db.use_in_memory_system();
        assert!(cache.copy_if_matching(&mut db, &key, destination)?);
        assert_eq!(db.system().read_to_string(&copied)?, "value = 1\n");
        assert!(!db.system().path_exists(&destination.join("test_only.py")));

        fs::write(site_packages.join("broken.py"), [0xff])?;
        assert!(cache.copy_if_matching(&mut db, &key, destination).is_err());
        assert!(cache.entry.is_none());
        Ok(())
    }
}
