use std::collections::hash_map::DefaultHasher;
use std::fs::{create_dir_all, File, Metadata};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use cacache::Error::EntryNotFound;
use filetime::FileTime;
use log::error;
use path_absolutize::Absolutize;
use serde::{Deserialize, Serialize};

use crate::autofix::fixer;
use crate::message::Message;
use crate::settings::Settings;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

fn cache_key(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> String {
    let mut hasher = DefaultHasher::new();
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
    format!(
        "{}@{}@{}",
        path.absolutize().unwrap().to_string_lossy(),
        VERSION,
        hasher.finish()
    )
}

pub fn init() -> Result<()> {
    let gitignore_path = Path::new(cache_dir()).join(".gitignore");
    if gitignore_path.exists() {
        return Ok(());
    }
    create_dir_all(cache_dir())?;
    let mut file = File::create(gitignore_path)?;
    file.write_all(b"*").map_err(|e| e.into())
}

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

    match cacache::read_sync(cache_dir(), cache_key(path, settings, autofix)) {
        Ok(encoded) => match bincode::deserialize::<CheckResult>(&encoded[..]) {
            Ok(CheckResult {
                metadata: CacheMetadata { mtime },
                messages,
            }) => {
                if FileTime::from_last_modification_time(metadata).unix_seconds() == mtime {
                    return Some(messages);
                }
            }
            Err(e) => error!("Failed to deserialize encoded cache entry: {e:?}"),
        },
        Err(EntryNotFound(_, _)) => {}
        Err(e) => error!("Failed to read from cache: {e:?}"),
    }
    None
}

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
    if let Err(e) = cacache::write_sync(
        cache_dir(),
        cache_key(path, settings, autofix),
        bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}")
    }
}
