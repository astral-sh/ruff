use std::path::Path;

use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::check::check_ast;
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
    // TODO(charlie): These specific files are causing a stack overflow.
    if path.to_string_lossy().eq_ignore_ascii_case(
         "../spring-experiments/spr_experiments/spr_experiments/assayworks/experiments/order_20220204/pipeline_steps.py") ||
        path.to_string_lossy().eq_ignore_ascii_case
        ( "../spring-experiments/spr_platform/spr_platform/data_index/bigquery_index.py")
    {
        return Ok(vec![]);
    }

    // Check the cache.
    if let Some(messages) = cache::get(path) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Run the linter.
    let python_ast = parser::parse(path)?;
    let messages = check_ast(path, &python_ast);
    cache::set(path, &messages);

    Ok(messages)
}
