//! Implements helper functions for using vendored/format.rs
use std::fmt;

use crate::vendored::format::FormatParseError;

impl fmt::Display for FormatParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            FormatParseError::EmptyAttribute => "Empty attribute in format string",
            FormatParseError::InvalidCharacterAfterRightBracket => {
                "Only '.' or '[' may follow ']' in format field specifier"
            }
            FormatParseError::InvalidFormatSpecifier => "Invalid format specifier",
            FormatParseError::MissingStartBracket => "Single '}' encountered in format string",
            FormatParseError::MissingRightBracket => "Expected '}' before end of string",
            FormatParseError::UnmatchedBracket => "Expected '}' before end of string",
            _ => "Unexpected error parsing format string",
        };

        write!(f, "{message}")
    }
}
