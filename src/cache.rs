use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::fs::{create_dir_all, File, Metadata};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use log::error;
use path_absolutize::Absolutize;
use serde::{Deserialize, Serialize};

use crate::autofix::fixer;
use crate::message::Message;
use crate::settings::Settings;

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

pub enum Mode {
    ReadWrite,
    ReadOnly,
    WriteOnly,
    None,
}

impl Mode {
    fn allow_read(&self) -> bool {
        match self {
            Mode::ReadWrite => true,
            Mode::ReadOnly => true,
            Mode::WriteOnly => false,
            Mode::None => false,
        }
    }

    fn allow_write(&self) -> bool {
        match self {
            Mode::ReadWrite => true,
            Mode::ReadOnly => false,
            Mode::WriteOnly => true,
            Mode::None => false,
        }
    }
}

impl From<bool> for Mode {
    fn from(value: bool) -> Self {
        match value {
            true => Mode::ReadWrite,
            false => Mode::None,
        }
    }
}

fn cache_dir() -> &'static str {
    "./.ruff_cache"
}

fn content_dir() -> &'static str {
    "content"
}

fn cache_key(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> u64 {
    let mut hasher = DefaultHasher::new();
    CARGO_PKG_VERSION.hash(&mut hasher);
    path.absolutize().unwrap().hash(&mut hasher);
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
    hasher.finish()
}

/// Initialize the cache directory.
pub fn init() -> Result<()> {
    let path = Path::new(cache_dir());

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

fn write_sync(key: &u64, value: &[u8]) -> Result<(), std::io::Error> {
    fs::write(
        Path::new(cache_dir())
            .join(content_dir())
            .join(format!("{key:x}")),
        value,
    )
}

fn read_sync(key: &u64) -> Result<Vec<u8>, std::io::Error> {
    fs::read(
        Path::new(cache_dir())
            .join(content_dir())
            .join(format!("{key:x}")),
    )
}

/// Get a value from the cache.
pub fn get(
    path: &Path,
    metadata: &Metadata,
    settings: &Settings,
    autofix: &fixer::Mode,
    mode: &Mode,
) -> Option<Vec<Message>> {
    if !mode.allow_read() {
        return None;
    };

    if let Ok(encoded) = read_sync(&cache_key(path, settings, autofix)) {
        match bincode::deserialize::<CheckResult>(&encoded[..]) {
            Ok(CheckResult {
                metadata: CacheMetadata { mtime },
                messages,
            }) => {
                if FileTime::from_last_modification_time(metadata).unix_seconds() == mtime {
                    return Some(messages);
                }
            }
            Err(e) => error!("Failed to deserialize encoded cache entry: {e:?}"),
        }
    }
    None
}

/// Set a value in the cache.
pub fn set(
    path: &Path,
    metadata: &Metadata,
    settings: &Settings,
    autofix: &fixer::Mode,
    messages: &[Message],
    mode: &Mode,
) {
    if !mode.allow_write() {
        return;
    };

    let check_result = CheckResultRef {
        metadata: &CacheMetadata {
            mtime: FileTime::from_last_modification_time(metadata).unix_seconds(),
        },
        messages,
    };
    if let Err(e) = write_sync(
        &cache_key(path, settings, autofix),
        &bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}")
    }
}
