use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use log::error;
use path_absolutize::Absolutize;
use ruff::message::Message;
use ruff::settings::{flags, AllSettings, Settings};
use ruff_cache::{CacheKey, CacheKeyHasher};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct CheckResultRef<'a> {
    messages: &'a [Message],
}

#[derive(Deserialize)]
struct CheckResult {
    messages: Vec<Message>,
}

fn content_dir() -> &'static Path {
    Path::new("content")
}

fn cache_key<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    metadata: &fs::Metadata,
    settings: &Settings,
    autofix: flags::Autofix,
) -> u64 {
    let mut hasher = CacheKeyHasher::new();
    CARGO_PKG_VERSION.cache_key(&mut hasher);
    path.as_ref().absolutize().unwrap().cache_key(&mut hasher);
    package
        .as_ref()
        .map(|path| path.as_ref().absolutize().unwrap())
        .cache_key(&mut hasher);
    FileTime::from_last_modification_time(metadata).cache_key(&mut hasher);
    #[cfg(unix)]
    metadata.permissions().mode().cache_key(&mut hasher);
    settings.cache_key(&mut hasher);
    autofix.cache_key(&mut hasher);
    hasher.finish()
}

#[allow(dead_code)]
/// Initialize the cache at the specified `Path`.
pub fn init(path: &Path) -> Result<()> {
    // Create the cache directories.
    fs::create_dir_all(path.join(content_dir()))?;

    // Add the CACHEDIR.TAG.
    if !cachedir::is_tagged(path)? {
        cachedir::add_tag(path)?;
    }

    // Add the .gitignore.
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        let mut file = fs::File::create(gitignore_path)?;
        file.write_all(b"*")?;
    }

    Ok(())
}

fn write_sync(cache_dir: &Path, key: u64, value: &[u8]) -> Result<(), std::io::Error> {
    fs::write(
        cache_dir.join(content_dir()).join(format!("{key:x}")),
        value,
    )
}

fn read_sync(cache_dir: &Path, key: u64) -> Result<Vec<u8>, std::io::Error> {
    fs::read(cache_dir.join(content_dir()).join(format!("{key:x}")))
}

fn del_sync(cache_dir: &Path, key: u64) -> Result<(), std::io::Error> {
    fs::remove_file(cache_dir.join(content_dir()).join(format!("{key:x}")))
}

/// Get a value from the cache.
pub fn get<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
) -> Option<Vec<Message>> {
    let encoded = read_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
    )
    .ok()?;
    match bincode::deserialize::<CheckResult>(&encoded[..]) {
        Ok(CheckResult { messages }) => Some(messages),
        Err(e) => {
            error!("Failed to deserialize encoded cache entry: {e:?}");
            None
        }
    }
}

/// Set a value in the cache.
pub fn set<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
    messages: &[Message],
) {
    let check_result = CheckResultRef { messages };
    if let Err(e) = write_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
        &bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}");
    }
}

/// Delete a value from the cache.
pub fn del<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    metadata: &fs::Metadata,
    settings: &AllSettings,
    autofix: flags::Autofix,
) {
    drop(del_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, metadata, &settings.lib, autofix),
    ));
}
