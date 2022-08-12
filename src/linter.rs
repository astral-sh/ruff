use std::path::Path;

use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::checker::check_ast;
use crate::message::Message;
use crate::{cache, parser};

#[derive(Serialize, Deserialize)]
struct CacheMetadata {
    size: u64,
    mtime: i64,
}

#[derive(Serialize, Deserialize)]
struct CheckResult {
    metadata: CacheMetadata,
    messages: Vec<Message>,
}

pub fn check_path(path: &Path) -> Result<Vec<Message>> {
    // Check the cache.
    if let Some(messages) = cache::get(path) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Run the linter.
    let python_ast = parser::parse(path)?;
    let messages: Vec<Message> = check_ast(&python_ast)
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
        .collect();
    cache::set(path, &messages);

    Ok(messages)
}
