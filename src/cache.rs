use std::borrow::Cow;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use cacache::Error::EntryNotFound;
use log::error;
use serde::{Deserialize, Serialize};

use crate::message::Message;

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

fn cache_dir() -> &'static str {
    "./.cache"
}

fn cache_key(path: &Path) -> Cow<str> {
    path.to_string_lossy()
}

pub fn get(path: &Path) -> Option<Vec<Message>> {
    match cacache::read_sync(cache_dir(), cache_key(path)) {
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

pub fn set(path: &Path, messages: &[Message]) {
    if let Ok(metadata) = path.metadata() {
        let check_result = CheckResultRef {
            metadata: &CacheMetadata {
                size: metadata.size(),
                mtime: metadata.mtime(),
            },
            messages,
        };
        match cacache::write_sync(
            cache_dir(),
            cache_key(path),
            bincode::serialize(&check_result).unwrap(),
        ) {
            Ok(_) => {}
            Err(e) => error!("Failed to write to cache: {e:?}"),
        }
    }
}
