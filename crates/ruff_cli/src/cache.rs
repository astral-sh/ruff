use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use log::error;
use path_absolutize::Absolutize;
use ruff::message::Message;
use ruff::settings::{flags, AllSettings, Settings};
use serde::{Deserialize, Serialize};

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct CacheMetadata {
    mtime: i64,
}

#[derive(Serialize)]
struct CheckResultRef<'a> {
    metadata: &'a CacheMetadata,
    messages: &'a [Message],
}

#[derive(Deserialize)]
struct CheckResult {
    metadata: CacheMetadata,
    messages: Vec<Message>,
}

fn content_dir() -> &'static Path {
    Path::new("content")
}

fn cache_key<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    settings: &Settings,
    autofix: flags::Autofix,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    CARGO_PKG_VERSION.hash(&mut hasher);
    path.as_ref().absolutize().unwrap().hash(&mut hasher);
    package
        .as_ref()
        .map(|path| path.as_ref().absolutize().unwrap())
        .hash(&mut hasher);
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
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
        cache_key(path, package, &settings.lib, autofix),
    )
    .ok()?;
    let (mtime, messages) = match bincode::deserialize::<CheckResult>(&encoded[..]) {
        Ok(CheckResult {
            metadata: CacheMetadata { mtime },
            messages,
        }) => (mtime, messages),
        Err(e) => {
            error!("Failed to deserialize encoded cache entry: {e:?}");
            return None;
        }
    };
    if FileTime::from_last_modification_time(metadata).unix_seconds() != mtime {
        return None;
    }
    Some(messages)
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
    let check_result = CheckResultRef {
        metadata: &CacheMetadata {
            mtime: FileTime::from_last_modification_time(metadata).unix_seconds(),
        },
        messages,
    };
    if let Err(e) = write_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, &settings.lib, autofix),
        &bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}");
    }
}

/// Delete a value from the cache.
pub fn del<P: AsRef<Path>>(
    path: P,
    package: Option<&P>,
    settings: &AllSettings,
    autofix: flags::Autofix,
) {
    drop(del_sync(
        &settings.cli.cache_dir,
        cache_key(path, package, &settings.lib, autofix),
    ));
}
