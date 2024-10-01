//! Settings for the `pycodestyle` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

use crate::line_width::LineLength;

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub max_line_length: LineLength,
    pub max_doc_length: Option<LineLength>,
    pub ignore_overlong_task_comments: bool,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pycodestyle",
            fields = [
                self.max_line_length,
                self.max_doc_length | optional,
                self.ignore_overlong_task_comments,
            ]
        }
        Ok(())
    }
}
