use std::fmt;

use ruff_macros::CacheKey;

use crate::display_settings;

/// Statement ordering strategy used by ssort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CacheKey, Default)]
pub enum Order {
    /// Sort in newspaper order (most important information first).
    #[default]
    Newspaper,

    /// Sort in narrative order (definitions before usages).
    Narrative,
}

impl fmt::Display for Order {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Narrative => formatter.write_str("narrative"),
            Self::Newspaper => formatter.write_str("newspaper"),
        }
    }
}

/// The settings for the ssort rules.
#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    /// The statement ordering strategy.
    pub order: Order,
}

/// Allows Settings to be formatted for display.
impl fmt::Display for Settings {
    /// Format the settings for display.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = formatter,
            namespace = "linter.ssort",
            fields = [
                self.order
            ]
        }
        Ok(())
    }
}
