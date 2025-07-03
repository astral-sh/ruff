//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub use_indian_decimal_format: bool,
    pub hex_digit_group_size: usize,
    pub oct_digit_group_size: usize,
    pub bin_digit_group_size: usize,
    pub hex_digit_grouping_threshold: usize,
    pub dec_digit_grouping_threshold: usize,
    pub oct_digit_grouping_threshold: usize,
    pub bin_digit_grouping_threshold: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            parenthesize_tuple_in_subscript: false,
            use_indian_decimal_format: false,
            hex_digit_group_size: 4,
            oct_digit_group_size: 4,
            bin_digit_group_size: 8,
            hex_digit_grouping_threshold: 5,
            dec_digit_grouping_threshold: 5,
            oct_digit_grouping_threshold: 5,
            bin_digit_grouping_threshold: 8,
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.ruff",
            fields = [
                self.parenthesize_tuple_in_subscript,
                self.use_indian_decimal_format,
                self.bin_digit_grouping_threshold,
                self.oct_digit_grouping_threshold,
                self.hex_digit_grouping_threshold,
                self.dec_digit_grouping_threshold,
                self.bin_digit_group_size,
                self.oct_digit_group_size,
                self.hex_digit_group_size,
            ]
        }
        Ok(())
    }
}
