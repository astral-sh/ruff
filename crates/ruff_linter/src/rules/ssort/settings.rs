use std::fmt;

use ruff_macros::CacheKey;

use crate::display_settings;

/// The settings for the ssort rules.
#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    /// Whether to sort statements in narrative order (definitions before usages).
    pub narrative_order: bool,
}

/// Allows Settings to be formatted for display.
impl fmt::Display for Settings {
    /// Format the settings for display.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = formatter,
            namespace = "linter.ssort",
            fields = [
                self.narrative_order
            ]
        }
        Ok(())
    }
}
