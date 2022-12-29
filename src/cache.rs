use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::fs::{create_dir_all, File, Metadata};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use filetime::FileTime;
use log::error;
use once_cell::sync::Lazy;
use path_absolutize::Absolutize;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::settings::{flags, Settings};

static CACHE_DIR: Lazy<Option<String>> = Lazy::new(|| std::env::var("RUFF_CACHE_DIR").ok());
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

/// Return the cache directory for a given project root. Defers to the
/// `RUFF_CACHE_DIR` environment variable, if set.
pub fn cache_dir(project_root: &Path) -> PathBuf {
    CACHE_DIR
        .as_ref()
        .map_or_else(|| project_root.join(".ruff_cache"), PathBuf::from)
}

fn content_dir() -> &'static Path {
    Path::new("content")
}

fn cache_key<P: AsRef<Path>>(path: P, settings: &Settings, autofix: flags::Autofix) -> u64 {
    let mut hasher = DefaultHasher::new();
    CARGO_PKG_VERSION.hash(&mut hasher);
    path.as_ref().absolutize().unwrap().hash(&mut hasher);
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
    hasher.finish()
}

/// Initialize the cache at the specified `Path`.
pub fn init(path: &Path) -> Result<()> {
    // Create the cache directories.
    create_dir_all(path.join(content_dir()))?;

    // Add the CACHEDIR.TAG.
    if !cachedir::is_tagged(path)? {
        cachedir::add_tag(path)?;
    }

    // Add the .gitignore.
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        let mut file = File::create(gitignore_path)?;
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

/// Get a value from the cache.
pub fn get<P: AsRef<Path>>(
    path: P,
    metadata: &Metadata,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Vec<Message>> {
    let encoded = read_sync(&settings.cache_dir, cache_key(path, settings, autofix)).ok()?;
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
    metadata: &Metadata,
    settings: &Settings,
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
        &settings.cache_dir,
        cache_key(path, settings, autofix),
        &bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}");
    }
}
