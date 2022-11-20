// cacache uses asyncd-std which has no wasm support, so currently no caching
// support on wasm
#![cfg_attr(
    target_family = "wasm",
    allow(unused_imports, unused_variables, dead_code)
)]

use std::collections::hash_map::DefaultHasher;
use std::fs::{create_dir_all, File, Metadata};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use filetime::FileTime;
use log::error;
use path_absolutize::{path_dedot, Absolutize};
use rustpython_common::vendored::ascii::AsciiChar::h;
use serde::{Deserialize, Serialize};

use crate::autofix::fixer;
use crate::cache;
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

fn cache_key(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> u64 {
    let mut hasher = DefaultHasher::new();
    CARGO_PKG_VERSION.hash(&mut hasher);
    path.hash(&mut hasher);
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
    hasher.finish()
}

/// Initialize the cache directory.
pub fn init() -> Result<()> {
    let path = Path::new(cache_dir());

    // Create the directory.
    create_dir_all(path)?;

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

fn write_sync<T: serde::Serialize>(key: &u64, value: &T) -> Result<(), Box<bincode::ErrorKind>> {
    // println!("{:?}", path_dedot::CWD);
    // println!("{:?}", cache_dir());
    // println!("{:?}", key);
    // println!("{:?}", path_dedot::CWD.join(cache_dir()).join(key));
    bincode::serialize_into(
        File::create(Path::new(cache_dir()).join(key.to_string()))?,
        value,
    )
}

fn read_sync<T: serde::de::DeserializeOwned>(key: &u64) -> Result<T, Box<bincode::ErrorKind>> {
    // println!("{:?}", path_dedot::CWD);
    // println!("{:?}", cache_dir());
    // println!("{:?}", key);
    // println!("{:?}", Path::new(cache_dir()).join(key));
    bincode::deserialize_from(File::open(Path::new(cache_dir()).join(key.to_string()))?)
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

    #[cfg(not(target_family = "wasm"))] // cacache needs async-std which doesn't support wasm
    match read_sync::<CheckResult>(&cache_key(path, settings, autofix)) {
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

    #[cfg(not(target_family = "wasm"))] // modification date not supported on wasm
    let check_result = CheckResultRef {
        metadata: &CacheMetadata {
            mtime: FileTime::from_last_modification_time(metadata).unix_seconds(),
        },
        messages,
    };
    #[cfg(not(target_family = "wasm"))] // cacache needs async-std which doesn't support wasm
    if let Err(e) = write_sync(&cache_key(path, settings, autofix), &check_result) {
        error!("Failed to write to cache: {e:?}")
    }
}
