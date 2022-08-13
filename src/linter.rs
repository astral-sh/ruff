use std::path::Path;

use anyhow::Result;
use log::debug;

use crate::checker::check_ast;
use crate::message::Message;
use crate::{cache, parser};

pub fn check_path(path: &Path, mode: &cache::Mode) -> Result<Vec<Message>> {
    // Check the cache.
    if let Some(messages) = cache::get(path, mode) {
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
    cache::set(path, &messages, mode);

    Ok(messages)
}
