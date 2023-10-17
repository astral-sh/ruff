//! Settings for the `pycodestyle` plugin.

use ruff_macros::CacheKey;

use crate::line_width::LineWidth;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub max_doc_width: Option<LineWidth>,
    pub ignore_overlong_task_comments: bool,
}
