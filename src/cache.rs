use std::borrow::Cow;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

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
    if let Ok(encoded) = cacache::read_sync(cache_dir(), cache_key(path)) {
        if let Ok(file_metadata) = path.metadata() {
            if let Ok(CheckResult { metadata, messages }) =
                bincode::deserialize::<CheckResult>(&encoded[..])
            {
                if file_metadata.size() == metadata.size && file_metadata.mtime() == metadata.mtime
                {
                    return Some(messages);
                }
            }
        }
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
        let _ = cacache::write_sync(
            cache_dir(),
            cache_key(path),
            bincode::serialize(&check_result).unwrap(),
        );
    }
}
