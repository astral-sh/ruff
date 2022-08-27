use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use cacache::Error::EntryNotFound;
use log::error;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::settings::Settings;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct CacheMetadata {
    size: u64,
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
            false => Mode::WriteOnly,
        }
    }
}

fn cache_dir() -> &'static str {
    "./.cache"
}

fn cache_key(path: &Path, settings: &Settings) -> String {
    let mut hasher = DefaultHasher::new();
    settings.hash(&mut hasher);
    format!(
        "{}@{}@{}",
        path.canonicalize().unwrap().to_string_lossy(),
        VERSION,
        hasher.finish()
    )
}

pub fn get(path: &Path, settings: &Settings, mode: &Mode) -> Option<Vec<Message>> {
    if !mode.allow_read() {
        return None;
    };

    match cacache::read_sync(cache_dir(), cache_key(path, settings)) {
        Ok(encoded) => match path.metadata() {
            Ok(m) => match bincode::deserialize::<CheckResult>(&encoded[..]) {
                Ok(CheckResult { metadata, messages }) => {
                    if m.size() == metadata.size && m.mtime() == metadata.mtime {
                        return Some(messages);
                    }
                }
                Err(e) => error!("Failed to deserialize encoded cache entry: {e:?}"),
            },
            Err(e) => error!("Failed to read metadata from path: {e:?}"),
        },
        Err(EntryNotFound(_, _)) => {}
        Err(e) => error!("Failed to read from cache: {e:?}"),
    }
    None
}

pub fn set(path: &Path, settings: &Settings, messages: &[Message], mode: &Mode) {
    if !mode.allow_write() {
        return;
    };

    if let Ok(metadata) = path.metadata() {
        let check_result = CheckResultRef {
            metadata: &CacheMetadata {
                size: metadata.size(),
                mtime: metadata.mtime(),
            },
            messages,
        };
        if let Err(e) = cacache::write_sync(
            cache_dir(),
            cache_key(path, settings),
            bincode::serialize(&check_result).unwrap(),
        ) {
            error!("Failed to write to cache: {e:?}")
        }
    }
}
